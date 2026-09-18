//! Inspect committed storage without involving plugin binaries.
use crate::{operator::Operator, ui::Ui};
use anyhow::{Context, bail};
use omega_host::{
    Layout,
    storage::{Entry, EntryKey, Envelope, JsonStore, Lease},
};
use omega_proto::{omega::StorageQuery, storage::StorageId};
use std::collections::BTreeMap;

/// Inspect shared stores. Answers are JSON on stdout.
#[derive(Debug, clap::Args)]
pub struct StorageCmd {
    /// Read persistent files while the daemon is stopped; refuse a held storage lease.
    #[arg(long, global = true)]
    offline: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// List storage metadata, excluding values.
    List,
    /// Read one bounded page, in canonical key order.
    Show {
        id: StorageId,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long)]
        after: Option<String>,
    },
    /// Export one complete, consistent JSON envelope.
    Export { id: StorageId },
}

impl StorageCmd {
    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let answer = if self.offline {
            self.offline()?
        } else {
            self.online().await?
        };
        ui.line(serde_json::to_string_pretty(&answer)?);
        Ok(())
    }

    fn query(limit: u32, after: Option<String>) -> anyhow::Result<StorageQuery> {
        let query = StorageQuery {
            key: None,
            after,
            limit,
        };
        query.validate()?;
        Ok(query)
    }
    async fn online(&self) -> anyhow::Result<serde_json::Value> {
        let operator = Operator::new();
        match &self.command {
            Command::List => Ok(serde_json::from_str(&operator.storage_info(None).await?)?),
            Command::Show { id, limit, after } => {
                let page = operator
                    .storage_page(id, Self::query(*limit, after.clone())?)
                    .await?;
                Self::page_json(page)
            }
            Command::Export { id } => {
                let metadata: Vec<serde_json::Value> =
                    serde_json::from_str(&operator.storage_info(Some(id)).await?)?;
                let metadata = metadata.first().context("store metadata is missing")?;
                let mut envelope = Envelope {
                    id: id.clone(),
                    codec: metadata["codec"].as_str().context("missing codec")?.into(),
                    schema_version: metadata["schema_version"]
                        .as_u64()
                        .context("missing schema version")?
                        .try_into()?,
                    epoch: metadata["epoch"].as_str().context("missing epoch")?.into(),
                    revision: metadata["revision"].as_u64().context("missing revision")?,
                    entries: BTreeMap::new(),
                };
                let mut after = None;
                loop {
                    let page = operator.storage_page(id, Self::query(100, after)?).await?;
                    if page.revision.as_ref() != Some(&envelope.revision()) {
                        bail!(
                            "storage changed during export; retry to obtain a consistent snapshot"
                        );
                    }
                    if page.truncated && page.entries.is_empty() {
                        bail!("daemon returned an empty truncated page");
                    }
                    after = page.entries.last().map(|entry| entry.key.clone());
                    for entry in page.entries {
                        envelope.entries.insert(
                            EntryKey::try_from(entry.key)?,
                            Entry {
                                value: serde_json::from_slice(&entry.json)?,
                                revision: entry.revision,
                            },
                        );
                    }
                    if !page.truncated {
                        break;
                    }
                }
                Ok(serde_json::to_value(envelope)?)
            }
        }
    }

    fn page_json(page: omega_proto::omega::StoragePage) -> anyhow::Result<serde_json::Value> {
        let entries = page.entries.into_iter().map(|entry| {
            Ok(serde_json::json!({"key": entry.key, "value": serde_json::from_slice::<serde_json::Value>(&entry.json)?, "revision": entry.revision}))
        }).collect::<anyhow::Result<Vec<_>>>()?;
        Ok(
            serde_json::json!({"revision": page.revision, "entries": entries, "total": page.total, "truncated": page.truncated}),
        )
    }

    fn offline(&self) -> anyhow::Result<serde_json::Value> {
        let layout = Layout::resolve();
        let _lease = Lease::acquire(&layout)
            .context("cannot inspect offline storage; stop the daemon first")?;
        match &self.command {
            Command::List => {
                let files = match std::fs::read_dir(layout.storage_directory()) {
                    Ok(files) => files,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(serde_json::json!([]));
                    }
                    Err(error) => return Err(error.into()),
                };
                let mut values = BTreeMap::new();
                for entry in files {
                    let path = entry?.path();
                    if path.extension().is_none_or(|extension| extension != "json") {
                        continue;
                    }
                    let id: StorageId = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .context("non-UTF-8 storage filename")?
                        .parse()?;
                    let envelope = JsonStore::new(&layout, &id).inspect(&id)?;
                    values.insert(id.clone(), serde_json::json!({"id":id, "persistent":true,"epoch":envelope.epoch,"revision":envelope.revision,"schema_version":envelope.schema_version,"entries":envelope.entries.len()}));
                }
                Ok(serde_json::Value::Array(values.into_values().collect()))
            }
            Command::Show { id, limit, after } => Self::page_json(
                JsonStore::new(&layout, id)
                    .inspect(id)?
                    .page(&Self::query(*limit, after.clone())?),
            ),
            Command::Export { id } => Ok(serde_json::to_value(
                JsonStore::new(&layout, id).inspect(id)?,
            )?),
        }
    }
}
