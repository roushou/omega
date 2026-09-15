//! Resolve a retained tree's interaction without executing behavior.

use std::num::NonZeroU64;

use crate::Refusal;
use crate::omega::{Value, ViewTree, value};

/// An enabled, unambiguous binding in a rendered tree.
///
/// Resolution does not authorize a command or validate local capture ownership.
/// The caller must check instance identity, render freshness, and presentation
/// visibility before dispatch, and authorize the resolved behavior.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Interaction<'a> {
    Local(NonZeroU64),
    Command { command: &'a str, args: &'a [Value] },
}

impl<'a> Interaction<'a> {
    /// Resolve one node and event, inheriting `disabled` and `busy` from ancestors.
    ///
    /// Duplicate target keys are invalid even if one target is disabled or lacks
    /// the event. With a unique target, availability is checked before its event.
    /// Empty content and disabled controls return `FailedPrecondition`; missing
    /// nodes/events, ambiguous keys, and mixed local/command bindings return
    /// `InvalidArgument`. Unrelated duplicate keys do not affect this interaction.
    ///
    /// ```
    /// use omega_proto::{Interaction, omega::{Bind, ViewNode, ViewTree}};
    /// let tree = ViewTree {
    ///     root: Some(ViewNode {
    ///         key: "submit".into(),
    ///         events: [("press".into(), Bind { local: 1, ..Default::default() })].into(),
    ///         ..Default::default()
    ///     }),
    ///     ..Default::default()
    /// };
    /// assert!(matches!(Interaction::resolve(&tree, "submit", "press")?, Interaction::Local(_)));
    /// # Ok::<(), omega_proto::Refusal>(())
    /// ```
    pub fn resolve(tree: &'a ViewTree, key: &str, event: &str) -> Result<Self, Refusal> {
        let root = tree
            .root
            .as_ref()
            .ok_or_else(|| Refusal::precondition("instance has no content"))?;
        let mut stack = vec![(root, true)];
        let mut target = None;
        while let Some((node, enabled)) = stack.pop() {
            let enabled = enabled
                && !["disabled", "busy"].iter().any(|prop| {
                    node.props
                        .get(*prop)
                        .is_some_and(|v| matches!(v.kind, Some(value::Kind::BoolValue(true))))
                });
            if node.key == key && target.replace((node, enabled)).is_some() {
                return Err(Refusal::invalid("view has ambiguous node keys"));
            }
            stack.extend(node.children.iter().map(|child| (child, enabled)));
        }
        let (node, enabled) = target.ok_or_else(|| Refusal::invalid("unknown interaction node"))?;
        if !enabled {
            return Err(Refusal::precondition("control is disabled"));
        }
        let binding = node
            .events
            .get(event)
            .ok_or_else(|| Refusal::invalid("node has no binding for this event"))?;
        match NonZeroU64::new(binding.local) {
            Some(local) => {
                if !binding.command.is_empty() || !binding.args.is_empty() {
                    return Err(Refusal::invalid(
                        "local binding cannot carry command arguments",
                    ));
                }
                Ok(Self::Local(local))
            }
            None => Ok(Self::Command {
                command: &binding.command,
                args: &binding.args,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IntoValue;
    use crate::omega::{Bind, ErrorCode, ViewNode};

    struct Fixture;
    impl Fixture {
        fn target(binding: Bind) -> ViewNode {
            ViewNode {
                key: "target".into(),
                events: [("press".into(), binding)].into(),
                ..Default::default()
            }
        }
        fn tree(children: Vec<ViewNode>) -> ViewTree {
            ViewTree {
                root: Some(ViewNode {
                    children,
                    ..Default::default()
                }),
                ..Default::default()
            }
        }
        fn local() -> ViewNode {
            Self::target(Bind {
                local: 7,
                ..Default::default()
            })
        }
        fn refusal(tree: &ViewTree, code: ErrorCode) {
            assert_eq!(
                Interaction::resolve(tree, "target", "press")
                    .unwrap_err()
                    .code,
                code
            );
        }
    }

    #[test]
    fn resolves_local_and_borrowed_command_arguments() {
        let tree = Fixture::tree(vec![Fixture::local()]);
        assert_eq!(
            Interaction::resolve(&tree, "target", "press"),
            Ok(Interaction::Local(NonZeroU64::new(7).unwrap()))
        );
        let args = vec!["bound".into_value()];
        let tree = Fixture::tree(vec![Fixture::target(Bind {
            command: "activate".into(),
            args: args.clone(),
            local: 0,
        })]);
        assert_eq!(
            Interaction::resolve(&tree, "target", "press"),
            Ok(Interaction::Command {
                command: "activate",
                args: &args
            })
        );
    }

    #[test]
    fn availability_is_inherited_and_false_does_not_override_ancestors() {
        for prop in ["disabled", "busy"] {
            let mut target = Fixture::local();
            target.props.insert(prop.into(), false.into_value());
            let mut tree = Fixture::tree(vec![target]);
            tree.root
                .as_mut()
                .unwrap()
                .props
                .insert(prop.into(), true.into_value());
            Fixture::refusal(&tree, ErrorCode::FailedPrecondition);
            tree.root
                .as_mut()
                .unwrap()
                .props
                .insert(prop.into(), false.into_value());
            assert!(Interaction::resolve(&tree, "target", "press").is_ok());
            tree.root.as_mut().unwrap().children[0]
                .props
                .insert(prop.into(), true.into_value());
            Fixture::refusal(&tree, ErrorCode::FailedPrecondition);
        }
    }

    #[test]
    fn ambiguity_precedes_availability_and_event_checks_in_either_order() {
        let mut invalid = Fixture::local();
        invalid.props.insert("disabled".into(), true.into_value());
        invalid.events.clear();
        for children in [
            vec![invalid.clone(), Fixture::local()],
            vec![Fixture::local(), invalid],
        ] {
            let refusal =
                Interaction::resolve(&Fixture::tree(children), "target", "press").unwrap_err();
            assert_eq!(refusal, Refusal::invalid("view has ambiguous node keys"));
        }
        let tree = Fixture::tree(vec![
            Fixture::local(),
            ViewNode::default(),
            ViewNode::default(),
        ]);
        assert!(Interaction::resolve(&tree, "target", "press").is_ok());
    }

    #[test]
    fn missing_content_nodes_events_and_mixed_bindings_are_refused() {
        Fixture::refusal(&ViewTree::default(), ErrorCode::FailedPrecondition);
        Fixture::refusal(&Fixture::tree(vec![]), ErrorCode::InvalidArgument);
        let mut missing = Fixture::local();
        missing.events.clear();
        Fixture::refusal(&Fixture::tree(vec![missing]), ErrorCode::InvalidArgument);
        for binding in [
            Bind {
                local: 7,
                command: "activate".into(),
                ..Default::default()
            },
            Bind {
                local: 7,
                args: vec![Value::default()],
                ..Default::default()
            },
        ] {
            Fixture::refusal(
                &Fixture::tree(vec![Fixture::target(binding)]),
                ErrorCode::InvalidArgument,
            );
        }
    }
}
