use toml_edit::{DocumentMut, Item, Table, Value};

/// Gives a serialized document the shape its schema declares.
///
/// `toml_edit`'s serializer emits every nested map as an inline table, which
/// is unreadable for a document of any size. This promotes them to standard
/// `[section]` tables — except the entry tables a schema declares inline,
/// where `omega = { workspace = true }` is the shape everyone writes by
/// hand and every tool emits.
pub(super) struct Formatter<'a> {
    inline_entries: &'a [&'static str],
}

impl<'a> Formatter<'a> {
    pub(super) fn new(inline_entries: &'a [&'static str]) -> Self {
        Self { inline_entries }
    }

    pub(super) fn apply(&self, document: &mut DocumentMut) {
        self.format(document.as_table_mut(), "");
    }

    fn format(&self, table: &mut Table, path: &str) {
        for (key, item) in table.iter_mut() {
            let child = match path {
                "" => key.get().to_string(),
                parent => format!("{parent}.{key}"),
            };

            Self::promote(item);

            match item {
                // A declared table keeps inline entries and is not descended
                // into: its children are values, not sections.
                Item::Table(table) if self.inline_entries.contains(&child.as_str()) => {
                    Self::normalize(table);
                    Self::inline_entries(table);
                }
                Item::Table(table) => {
                    Self::normalize(table);
                    self.format(table, &child);
                }
                Item::ArrayOfTables(tables) => {
                    for table in tables.iter_mut() {
                        Self::normalize(table);
                        self.format(table, &child);
                    }
                }
                _ => {}
            }
        }
    }

    /// Turn a serialized inline table (or array of them) into a section (or a
    /// `[[section]]` array).
    fn promote(item: &mut Item) {
        let promoted = std::mem::take(item);
        let promoted = match promoted.into_table() {
            Ok(table) => Item::Table(table),
            Err(item) => item,
        };
        *item = match promoted.into_array_of_tables() {
            Ok(tables) => Item::ArrayOfTables(tables),
            Err(item) => item,
        };
    }

    /// Drop serializer decor, and let a table that holds only sections go
    /// unprinted — `[profile.release]` needs no `[profile]` above it.
    fn normalize(table: &mut Table) {
        table.decor_mut().clear();
        table.set_implicit(true);
    }

    fn inline_entries(table: &mut Table) {
        for (_, item) in table.iter_mut() {
            let Some(entry) = item.as_table() else {
                continue;
            };
            let mut inline = entry.clone().into_inline_table();
            inline.fmt();
            *item = Item::Value(Value::InlineTable(inline));
        }
    }
}
