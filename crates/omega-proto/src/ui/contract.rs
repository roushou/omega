use super::{NodeKind, Payload, Prop, PropKind};
use crate::omega::{Value, ViewNode, ViewTree, value};

/// Child cardinality and allowed child kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Children {
    None,
    Many,
    /// Zero or more named fields; form submission produces a map of strings.
    Fields,
    /// An optional single canvas. Compose multiple elements inside that canvas.
    Canvas,
}

impl Children {
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Many => "many",
            Self::Fields => "fields",
            Self::Canvas => "canvas",
        }
    }
}

/// A malformed known UI contract. Unknown vocabulary is retained for newer peers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("node {node:?}: {detail}")]
pub struct ContractError {
    pub node: String,
    pub detail: String,
}

impl ContractError {
    fn at(node: &ViewNode, detail: impl Into<String>) -> Self {
        Self {
            node: node.key.clone(),
            detail: detail.into(),
        }
    }
}

impl Prop {
    pub fn accepts(self, value: &Value) -> bool {
        let valid = match (self.kind, value.kind.as_ref()) {
            (PropKind::Text, Some(value::Kind::StringValue(_)))
            | (PropKind::Flag, Some(value::Kind::BoolValue(_))) => true,
            (PropKind::Number, _) => Payload::Unsigned.accepts(Some(value)),
            (PropKind::Fraction, _) => Payload::Fraction.accepts(Some(value)),
            (PropKind::Fractions, Some(value::Kind::List(list))) => list
                .values
                .iter()
                .all(|value| Payload::Fraction.accepts(Some(value))),
            _ => false,
        };
        valid
            && self.range.is_none_or(|(min, max)| {
                let number = match value.kind {
                    Some(value::Kind::IntValue(value)) => value as f64,
                    Some(value::Kind::DoubleValue(value)) => value,
                    _ => return false,
                };
                (min..=max).contains(&number)
            })
    }
}

impl ViewNode {
    /// Validate known properties and child structure, preserving unknown vocabulary.
    /// Unknown kinds are opaque to this build, including their descendants.
    pub fn validate_contract(&self) -> Result<(), ContractError> {
        self.validate(false)
    }

    /// Validate a view authored by this SDK version, including vocabulary coverage.
    /// Undeclared kinds, properties, and events are errors; shortcut events are valid.
    pub fn validate_authoring(&self) -> Result<(), ContractError> {
        self.validate(true)
    }

    fn validate(&self, authored: bool) -> Result<(), ContractError> {
        let Some(kind) = NodeKind::from_name(&self.r#type) else {
            return if authored {
                Err(ContractError::at(
                    self,
                    format!("undeclared node kind {:?}", self.r#type),
                ))
            } else {
                Ok(())
            };
        };
        if authored {
            for name in self.props.keys() {
                if !kind.every_prop().any(|prop| prop.name == name) {
                    return Err(ContractError::at(
                        self,
                        format!("undeclared property {name:?}"),
                    ));
                }
            }
            for event in self.events.keys() {
                if self.event_payload(event).is_none() {
                    return Err(ContractError::at(
                        self,
                        format!("undeclared event {event:?}"),
                    ));
                }
            }
        }
        for prop in kind.every_prop() {
            if self
                .props
                .get(prop.name)
                .is_some_and(|value| !prop.accepts(value))
            {
                return Err(ContractError::at(
                    self,
                    format!("invalid property {:?}", prop.name),
                ));
            }
        }
        match kind.children() {
            Children::None if !self.children.is_empty() => {
                return Err(ContractError::at(self, "this kind has no children"));
            }
            Children::Canvas if self.children.len() > 1 => {
                return Err(ContractError::at(
                    self,
                    "viewport accepts at most one canvas; compose its contents in a layout",
                ));
            }
            Children::Fields => {
                let mut names = std::collections::BTreeSet::new();
                for child in &self.children {
                    let name = child.props.get("name").and_then(|value| match &value.kind {
                        Some(value::Kind::StringValue(name)) => Some(name),
                        _ => None,
                    });
                    if child.r#type != "field"
                        || name.is_none_or(|name| name.is_empty() || !names.insert(name))
                    {
                        return Err(ContractError::at(
                            self,
                            "form children must be fields with unique nonempty names",
                        ));
                    }
                }
            }
            _ => {}
        }
        for child in &self.children {
            child.validate(authored)?;
        }
        Ok(())
    }

    /// Resolve the payload shape of a known native event or declared shortcut.
    pub fn event_payload(&self, event: &str) -> Option<Payload> {
        if self
            .shortcuts
            .iter()
            .any(|shortcut| shortcut.event == event)
        {
            return Some(Payload::None);
        }
        NodeKind::from_name(&self.r#type)?
            .events()
            .iter()
            .find(|contract| contract.name == event)
            .map(|contract| contract.payload)
    }

    /// Validate the supplied control value without interpreting unknown events.
    pub fn validate_event(&self, event: &str, value: Option<&Value>) -> Result<(), ContractError> {
        if self
            .event_payload(event)
            .is_some_and(|payload| !payload.accepts(value))
        {
            return Err(ContractError::at(
                self,
                format!("invalid payload for event {event:?}"),
            ));
        }
        Ok(())
    }
}

impl ViewTree {
    pub fn validate_contract(&self) -> Result<(), ContractError> {
        if let Some(root) = &self.root {
            root.validate_contract()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::omega::{Bind, ErrorCode};
    use crate::{Interaction, IntoValue, Values};

    struct Fixture;
    impl Fixture {
        fn node(kind: &str) -> ViewNode {
            ViewNode {
                r#type: kind.into(),
                key: "target".into(),
                ..Default::default()
            }
        }
    }

    #[test]
    fn known_properties_are_typed_bounded_and_finite_but_unknown_vocabulary_survives() {
        for (kind, property, value) in [
            ("field", "value", 1.0.into_value()),
            ("slider", "value", f64::NAN.into_value()),
            ("slider", "value", 2.0.into_value()),
            ("viewport", "zoom", 0.0.into_value()),
            ("viewport", "zoom", 9.0.into_value()),
            ("grid", "columns", 0_u32.into_value()),
            ("text", "width", (-1_i64).into_value()),
            ("graph", "points", vec![f64::INFINITY].into_value()),
        ] {
            let mut node = Fixture::node(kind);
            node.props.insert(property.into(), value);
            assert!(node.validate_contract().is_err(), "{kind}.{property}");
        }
        let mut node = Fixture::node("text");
        node.props
            .insert("future_property".into(), Value::default());
        node.events.insert("future_event".into(), Bind::default());
        assert!(node.validate_contract().is_ok());
        assert!(node.validate_authoring().is_err());
        assert!(node.validate_event("future_event", None).is_ok());
        node.r#type = "future_node".into();
        node.children.push(Fixture::node("viewport"));
        assert!(node.validate_contract().is_ok());
        assert!(node.validate_authoring().is_err());
    }

    #[test]
    fn child_rules_reject_silently_undrawn_content_and_ambiguous_form_fields() {
        let mut leaf = Fixture::node("text");
        leaf.children.push(Fixture::node("text"));
        assert!(leaf.validate_contract().is_err());
        let mut viewport = Fixture::node("viewport");
        assert!(viewport.validate_contract().is_ok());
        viewport.children.push(Fixture::node("stack"));
        assert!(viewport.validate_contract().is_ok());
        viewport.children.push(Fixture::node("text"));
        assert!(viewport.validate_contract().is_err());
        let mut form = Fixture::node("form");
        form.children.push(Fixture::node("text"));
        assert!(form.validate_contract().is_err());
        form.children[0] = Fixture::node("field");
        assert!(form.validate_contract().is_err());
        form.children[0]
            .props
            .insert("name".into(), "query".into_value());
        assert!(form.validate_contract().is_ok());
        form.children.push(form.children[0].clone());
        assert!(form.validate_contract().is_err());
    }

    #[test]
    fn interaction_validation_uses_the_retained_nodes_event_shape() {
        let mut node = Fixture::node("field");
        node.events.insert(
            "change".into(),
            Bind {
                local: 1,
                ..Default::default()
            },
        );
        let tree = ViewTree {
            root: Some(node),
            ..Default::default()
        };
        let edit = Values::new()
            .with("text", "draft")
            .with("revision", u32::MAX)
            .with("reset", 0_u32)
            .into_value();
        assert!(Interaction::resolve_value(&tree, "target", "change", Some(&edit)).is_ok());
        for value in [
            None,
            Some(Value::default()),
            Some("draft".into_value()),
            Some(
                Values::new()
                    .with("text", "draft")
                    .with("revision", 1.0)
                    .with("reset", 0_u32)
                    .into_value(),
            ),
        ] {
            let error =
                Interaction::resolve_value(&tree, "target", "change", value.as_ref()).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument);
        }
    }

    #[test]
    fn defaults_and_native_event_names_are_consistent() {
        use super::super::DefaultValue;
        for kind in NodeKind::ALL {
            let mut events = std::collections::BTreeSet::new();
            for event in kind.events() {
                assert!(events.insert(event.name), "{kind}");
            }
            for prop in kind.every_prop() {
                let value = match prop.fallback {
                    DefaultValue::Absent => continue,
                    DefaultValue::Text(value) => value.into_value(),
                    DefaultValue::Integer(value) => value.into_value(),
                    DefaultValue::Fraction(value) => value.into_value(),
                    DefaultValue::Flag(value) => value.into_value(),
                    DefaultValue::EmptyList => Vec::<f64>::new().into_value(),
                };
                assert!(prop.accepts(&value), "{kind}.{} default", prop.name);
            }
        }
    }
}
