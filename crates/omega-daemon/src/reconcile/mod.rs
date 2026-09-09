//! Converging the machine toward the state document.
//!
//! A reconciler is not a script. Each provider owns one domain, *plans* what
//! would change without touching anything, and only then applies it — so a
//! change can be shown before it happens, and applying nothing is the normal
//! outcome of a machine that already matches its document.

pub mod bars;
pub mod config;
pub mod converger;
pub mod environment;
pub mod schedules;
pub mod units;

use async_trait::async_trait;

use omega_proto::omega::StateDocument;

pub use bars::BarProvider;
pub use config::ConfigProvider;
pub use converger::{Context, Converger, Work};
pub use environment::EnvironmentProvider;
pub use schedules::ScheduleProvider;
pub use units::UnitProvider;

/// What a change does to an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    Delete,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

/// One difference between the document and the machine.
///
/// `target` is the entity's stable id — the same id the document carries, so
/// a plan reads as a diff of declarations rather than of side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub action: Action,
    pub target: String,
    pub summary: String,
}

impl Change {
    pub fn new(action: Action, target: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            action,
            target: target.into(),
            summary: summary.into(),
        }
    }

    pub fn create(target: impl Into<String>, summary: impl Into<String>) -> Self {
        Self::new(Action::Create, target, summary)
    }

    pub fn update(target: impl Into<String>, summary: impl Into<String>) -> Self {
        Self::new(Action::Update, target, summary)
    }

    pub fn delete(target: impl Into<String>, summary: impl Into<String>) -> Self {
        Self::new(Action::Delete, target, summary)
    }
}

impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.action.as_str(),
            self.target,
            self.summary
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{domain}: {message}")]
pub struct ProviderError {
    pub domain: &'static str,
    pub message: String,
}

impl ProviderError {
    pub fn new(domain: &'static str, message: impl Into<String>) -> Self {
        Self {
            domain,
            message: message.into(),
        }
    }
}

/// One domain of the machine that a document can describe.
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    /// What this provider converges, named for logs and plans.
    fn domain(&self) -> &'static str;

    /// The changes needed to match the document. Pure by contract: a plan is
    /// computed, shown, and only then applied.
    fn plan(&self, document: &StateDocument) -> Vec<Change>;

    /// Apply a plan this provider produced. The document comes with it: the
    /// plan says what must change, the document says what to.
    async fn apply(
        &self,
        document: &StateDocument,
        changes: &[Change],
    ) -> Result<(), ProviderError>;
}

/// Every provider's plan for one document.
#[derive(Debug, Default)]
pub struct Plan {
    domains: Vec<(&'static str, Vec<Change>)>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.domains.iter().all(|(_, changes)| changes.is_empty())
    }

    pub fn changes(&self) -> impl Iterator<Item = (&'static str, &Change)> {
        self.domains
            .iter()
            .flat_map(|(domain, changes)| changes.iter().map(move |change| (*domain, change)))
    }

    pub fn len(&self) -> usize {
        self.domains.iter().map(|(_, changes)| changes.len()).sum()
    }
}

/// The set of providers a daemon converges with.
pub struct Reconciler {
    providers: Vec<Box<dyn Provider>>,
}

impl std::fmt::Debug for Reconciler {
    /// The domains it converges, since the providers themselves are trait
    /// objects and their innards are their own business.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reconciler")
            .field(
                "domains",
                &self
                    .providers
                    .iter()
                    .map(|provider| provider.domain())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Reconciler {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn with(mut self, provider: impl Provider) -> Self {
        self.providers.push(Box::new(provider));
        self
    }

    /// What would change, asked of every provider. No side effects.
    pub fn plan(&self, document: &StateDocument) -> Plan {
        Plan {
            domains: self
                .providers
                .iter()
                .map(|provider| (provider.domain(), provider.plan(document)))
                .collect(),
        }
    }

    /// Plan, then apply. A provider that fails does not stop the others: a
    /// machine converged in three domains out of four is closer to its
    /// document than one that gave up at the first error.
    pub async fn converge(&self, document: &StateDocument) -> Plan {
        let plan = self.plan(document);

        if plan.is_empty() {
            tracing::info!("machine already matches the document");
            return plan;
        }

        for (domain, change) in plan.changes() {
            tracing::info!(domain, "{change}");
        }

        for (provider, (domain, changes)) in self.providers.iter().zip(plan.domains.iter()) {
            if changes.is_empty() {
                continue;
            }
            if let Err(e) = provider.apply(document, changes).await {
                tracing::error!(domain, error = %e, "could not converge");
            }
        }

        plan
    }
}

impl Default for Reconciler {
    fn default() -> Self {
        Self::new()
    }
}
