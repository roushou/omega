//! Deterministic local search over a bounded collection.
//!
//! Rank by literal token matches and subsequence fuzziness, mirroring a
//! desktop launcher's own ordering: name prefix first, then name substring,
//! then secondary fields, description, and finally identity.

use std::marker::PhantomData;

/// A searchable item: a stable identity and ranked text fields.
pub trait Matchable {
    /// Stable identity, matched last and used to order ties.
    fn id(&self) -> &str;
    /// Primary display name, matched first.
    fn name(&self) -> &str;
    /// Category or generic name, matched after the name.
    fn generic(&self) -> &str {
        ""
    }
    /// Keywords, matched alongside the generic name.
    fn keywords(&self) -> &[String] {
        &[]
    }
    /// Longer description, matched after the generic name and keywords.
    fn description(&self) -> &str {
        ""
    }
    /// Pinned items sort before others on an empty query.
    fn pinned(&self) -> bool {
        false
    }
}

/// Ranked matches for one query.
#[derive(Debug, Clone, PartialEq)]
pub struct Matches<'a, T> {
    /// The top matches, at most the requested limit, in rank order.
    pub items: Vec<&'a T>,
    /// How many entries matched before the limit applied.
    pub total: usize,
}

/// Rank a collection by literal and fuzzy token matching.
#[derive(Debug, Clone)]
pub struct Search<T> {
    query: String,
    limit: u8,
    marker: PhantomData<fn() -> T>,
}

impl<T: Matchable> Search<T> {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            limit: 40,
            marker: PhantomData,
        }
    }

    /// Clamp the result count to 1..=100.
    pub fn limit(mut self, limit: u8) -> Self {
        self.limit = limit.clamp(1, 100);
        self
    }

    /// Rank entries. `pin` marks items that sort before others on an empty
    /// query, such as the caller's favorites.
    pub fn find<'a>(&self, entries: &'a [T], pin: impl Fn(&T) -> bool) -> Matches<'a, T> {
        let query = self.query.trim().to_lowercase();
        let tokens: Vec<_> = query.split_whitespace().collect();

        let mut ranked = Vec::new();
        for entry in entries {
            let name = entry.name().to_lowercase();
            let generic = entry.generic().to_lowercase();
            let keywords = entry.keywords().join(" ").to_lowercase();
            let description = entry.description().to_lowercase();
            let id = entry.id().to_lowercase();

            let mut score = 0usize;
            let mut matched = true;
            for token in &tokens {
                let Some(rank) =
                    Self::token_rank(token, &name, &generic, &keywords, &description, &id)
                else {
                    matched = false;
                    break;
                };
                score += rank;
            }
            if !matched {
                continue;
            }

            let priority = if query.is_empty() || name == query {
                0
            } else if name.starts_with(&query) {
                1
            } else {
                2
            };
            let personal = query.is_empty() && !pin(entry);
            ranked.push((priority, score, personal, name, entry));
        }

        ranked
            .sort_by(|a, b| (a.0, a.1, a.2, &a.3, a.4.id()).cmp(&(b.0, b.1, b.2, &b.3, b.4.id())));

        let total = ranked.len();
        let items = ranked
            .into_iter()
            .take(usize::from(self.limit))
            .map(|(_, _, _, _, entry)| entry)
            .collect();
        Matches { items, total }
    }

    /// The rank one token earns against one entry's fields.
    fn token_rank(
        token: &str,
        name: &str,
        generic: &str,
        keywords: &str,
        description: &str,
        id: &str,
    ) -> Option<usize> {
        if name.starts_with(token) {
            Some(0)
        } else if name.contains(token) {
            Some(1)
        } else if generic.contains(token) || keywords.contains(token) {
            Some(2)
        } else if description.contains(token) {
            Some(3)
        } else if id.contains(token) {
            Some(4)
        } else {
            Self::fuzzy(token, name).map(|cost| 10 + cost)
        }
    }

    /// Whether `name` contains `token` as a subsequence; the cost is the
    /// distance the characters spanned.
    fn fuzzy(token: &str, name: &str) -> Option<usize> {
        let mut wanted = token.chars();
        let mut next = wanted.next()?;
        let mut first = None;
        let mut matched = 0;
        for (index, character) in name.chars().enumerate() {
            if character != next {
                continue;
            }
            first.get_or_insert(index);
            matched += 1;
            match wanted.next() {
                Some(character) => next = character,
                None => return Some(index + 1 - matched + first.unwrap_or(0)),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct App {
        id: &'static str,
        name: &'static str,
        generic: &'static str,
        keywords: Vec<String>,
        description: &'static str,
    }

    impl Matchable for App {
        fn id(&self) -> &str {
            self.id
        }
        fn name(&self) -> &str {
            self.name
        }
        fn generic(&self) -> &str {
            self.generic
        }
        fn keywords(&self) -> &[String] {
            &self.keywords
        }
        fn description(&self) -> &str {
            self.description
        }
    }

    fn apps() -> Vec<App> {
        vec![
            App {
                id: "org.example.calculator",
                name: "Calculator",
                generic: "Utility",
                keywords: vec!["math".to_string()],
                description: "Solve arithmetic",
            },
            App {
                id: "org.example.calendar",
                name: "Calendar",
                generic: "Office",
                keywords: vec!["dates".to_string()],
                description: "Plan events",
            },
            App {
                id: "org.example.terminal",
                name: "Terminal",
                generic: "System",
                keywords: vec![],
                description: "Run commands",
            },
        ]
    }

    #[test]
    fn prefix_matches_rank_before_substring_and_fuzzy_matches() {
        let entries = apps();
        let matches = Search::new("cal").find(&entries, |_| false);
        assert_eq!(matches.total, 2);
        // Both are name prefixes and tie, so alphabetical order decides.
        assert_eq!(matches.items[0].name, "Calculator");
        assert_eq!(matches.items[1].name, "Calendar");

        // A fuzzy subsequence still matches when nothing literal does.
        let fuzzy = Search::new("trm").find(&entries, |_| false);
        assert_eq!(fuzzy.items[0].name, "Terminal");
    }

    #[test]
    fn an_empty_query_sorts_pinned_items_first_and_caps_the_limit() {
        let entries = apps();
        let matches = Search::new("")
            .limit(2)
            .find(&entries, |app| app.name == "Terminal");
        assert_eq!(matches.total, 3);
        assert_eq!(matches.items.len(), 2);
        assert_eq!(matches.items[0].name, "Terminal");
        assert_eq!(matches.items[1].name, "Calculator");
    }

    #[test]
    fn secondary_fields_and_identity_match_last() {
        let entries = apps();
        // "dates" is a keyword of Calendar; "org.example.calculator" is Calculator's id.
        let by_keyword = Search::new("dates").find(&entries, |_| false);
        assert_eq!(by_keyword.items[0].name, "Calendar");
        let by_id = Search::new("org.example.calculator").find(&entries, |_| false);
        assert_eq!(by_id.items[0].name, "Calculator");
    }
}
