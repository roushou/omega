use toml_edit::{Item, TableLike};

/// A malformed Cargo field or an edit that conflicts with an existing declaration.
#[derive(Debug, thiserror::Error)]
#[error("{field}: {reason}")]
pub struct CargoError {
    pub field: String,
    pub reason: String,
}

impl CargoError {
    pub(super) fn new(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
        }
    }
}

pub(super) struct Fields;

impl Fields {
    pub(super) fn table<'a>(
        parent: &'a dyn TableLike,
        key: &str,
        path: &str,
    ) -> Result<Option<&'a dyn TableLike>, CargoError> {
        parent
            .get(key)
            .map(|item| {
                item.as_table_like()
                    .ok_or_else(|| CargoError::new(path, "must be a table"))
            })
            .transpose()
    }

    pub(super) fn string<'a>(
        parent: &'a dyn TableLike,
        key: &str,
        path: &str,
    ) -> Result<Option<&'a str>, CargoError> {
        parent
            .get(key)
            .map(|item| {
                item.as_str()
                    .ok_or_else(|| CargoError::new(path, "must be a string"))
            })
            .transpose()
    }

    pub(super) fn strings<'a>(
        parent: &'a dyn TableLike,
        key: &str,
        path: &str,
    ) -> Result<Vec<&'a str>, CargoError> {
        let Some(item) = parent.get(key) else {
            return Ok(Vec::new());
        };
        let array = item
            .as_array()
            .ok_or_else(|| CargoError::new(path, "must be an array"))?;
        array
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| CargoError::new(path, "must contain strings"))
            })
            .collect()
    }

    pub(super) fn replace(item: &mut Item, replacement: Item) {
        let decor = item.as_value().map(|value| value.decor().clone());
        *item = replacement;
        if let (Some(decor), Some(value)) = (decor, item.as_value_mut()) {
            *value.decor_mut() = decor;
        }
    }
}
