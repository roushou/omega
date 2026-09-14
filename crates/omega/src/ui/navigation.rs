//! Component-scoped node references.

use std::collections::HashMap;

use super::Node;

/// Invalid IDs or references in a completed view.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ViewError {
    #[error("node {node:?} has an empty ID")]
    EmptyId { node: String },
    #[error("duplicate ID {id:?} on nodes {first:?} and {second:?} in the same component scope")]
    DuplicateId {
        id: String,
        first: String,
        second: String,
    },
    #[error("node {node:?} references missing ID {id:?} in its component scope")]
    MissingId { node: String, id: String },
    #[error("node {node:?} navigates to ID {id:?}, which is {kind}, not a list")]
    InvalidNavigation {
        node: String,
        id: String,
        kind: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Id(String);

struct Destination {
    key: String,
    kind: &'static str,
}

#[derive(Default)]
pub(super) struct References {
    scopes: Vec<HashMap<Id, Destination>>,
}

impl References {
    pub(super) fn resolve(root: &mut Node) -> Result<(), ViewError> {
        let mut references = Self {
            scopes: vec![HashMap::new()],
        };
        references.collect(root, 0)?;
        references.bind(root, 0, &mut 0)
    }

    fn collect(&mut self, node: &Node, parent: usize) -> Result<(), ViewError> {
        if let Some(id) = &node.id {
            let key = node.key.clone().unwrap();
            if id.is_empty() {
                return Err(ViewError::EmptyId { node: key });
            }
            let destination = Destination {
                key: key.clone(),
                kind: node.kind,
            };
            if let Some(first) = self.scopes[parent].insert(Id(id.clone()), destination) {
                return Err(ViewError::DuplicateId {
                    id: id.clone(),
                    first: first.key,
                    second: key,
                });
            }
        }
        let scope = if node.scope {
            self.scopes.push(HashMap::new());
            self.scopes.len() - 1
        } else {
            parent
        };
        for child in &node.children {
            self.collect(child, scope)?;
        }
        Ok(())
    }

    fn bind(&self, node: &mut Node, parent: usize, next: &mut usize) -> Result<(), ViewError> {
        let scope = if node.scope {
            *next += 1;
            *next
        } else {
            parent
        };
        if let Some(id) = &node.navigation {
            let key = node.key.clone().unwrap();
            let target =
                self.scopes[scope]
                    .get(&Id(id.clone()))
                    .ok_or_else(|| ViewError::MissingId {
                        node: key.clone(),
                        id: id.clone(),
                    })?;
            if target.kind != "list" {
                return Err(ViewError::InvalidNavigation {
                    node: key,
                    id: id.clone(),
                    kind: target.kind.into(),
                });
            }
            node.navigation_target = target.key.clone();
        }
        for child in &mut node.children {
            self.bind(child, scope, next)?;
        }
        Ok(())
    }
}
