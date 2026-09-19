//! Explicit dependency bundles; declaration and construction share the same types.
use super::{Wired, Wiring};
use crate::runtime::context::Context;
use omega_proto::omega::{Capability, CommandDependency, StorageDescriptor};
use omega_proto::{SystemTopic, Values};

impl<A: Wiring> Wired for (A,) {
    fn topics() -> Vec<SystemTopic> {
        [A::TOPICS.to_vec()].into_iter().flatten().collect()
    }
    fn capabilities() -> Vec<Capability> {
        [A::CAPABILITIES.to_vec()].into_iter().flatten().collect()
    }
    fn commands() -> Vec<CommandDependency> {
        [A::commands()].into_iter().flatten().collect()
    }
    fn required_topics() -> Vec<SystemTopic> {
        [A::required_topics()].into_iter().flatten().collect()
    }
    fn storage() -> Vec<StorageDescriptor> {
        [A::storage()].into_iter().flatten().collect()
    }
    fn keyspaces() -> Vec<String> {
        [A::keyspaces()].into_iter().flatten().collect()
    }
    fn build(context: &Context, _: &Values) -> Self {
        (A::build(context),)
    }
}
impl<A: Wiring, B: Wiring> Wired for (A, B) {
    fn topics() -> Vec<SystemTopic> {
        [A::TOPICS.to_vec(), B::TOPICS.to_vec()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn capabilities() -> Vec<Capability> {
        [A::CAPABILITIES.to_vec(), B::CAPABILITIES.to_vec()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn commands() -> Vec<CommandDependency> {
        [A::commands(), B::commands()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn required_topics() -> Vec<SystemTopic> {
        [A::required_topics(), B::required_topics()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn storage() -> Vec<StorageDescriptor> {
        [A::storage(), B::storage()].into_iter().flatten().collect()
    }
    fn keyspaces() -> Vec<String> {
        [A::keyspaces(), B::keyspaces()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn build(context: &Context, _: &Values) -> Self {
        (A::build(context), B::build(context))
    }
}
impl<A: Wiring, B: Wiring, C: Wiring> Wired for (A, B, C) {
    fn topics() -> Vec<SystemTopic> {
        [A::TOPICS.to_vec(), B::TOPICS.to_vec(), C::TOPICS.to_vec()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn capabilities() -> Vec<Capability> {
        [
            A::CAPABILITIES.to_vec(),
            B::CAPABILITIES.to_vec(),
            C::CAPABILITIES.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn commands() -> Vec<CommandDependency> {
        [A::commands(), B::commands(), C::commands()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn required_topics() -> Vec<SystemTopic> {
        [
            A::required_topics(),
            B::required_topics(),
            C::required_topics(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn storage() -> Vec<StorageDescriptor> {
        [A::storage(), B::storage(), C::storage()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn keyspaces() -> Vec<String> {
        [A::keyspaces(), B::keyspaces(), C::keyspaces()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn build(context: &Context, _: &Values) -> Self {
        (A::build(context), B::build(context), C::build(context))
    }
}
impl<A: Wiring, B: Wiring, C: Wiring, D: Wiring> Wired for (A, B, C, D) {
    fn topics() -> Vec<SystemTopic> {
        [
            A::TOPICS.to_vec(),
            B::TOPICS.to_vec(),
            C::TOPICS.to_vec(),
            D::TOPICS.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn capabilities() -> Vec<Capability> {
        [
            A::CAPABILITIES.to_vec(),
            B::CAPABILITIES.to_vec(),
            C::CAPABILITIES.to_vec(),
            D::CAPABILITIES.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn commands() -> Vec<CommandDependency> {
        [A::commands(), B::commands(), C::commands(), D::commands()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn required_topics() -> Vec<SystemTopic> {
        [
            A::required_topics(),
            B::required_topics(),
            C::required_topics(),
            D::required_topics(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn storage() -> Vec<StorageDescriptor> {
        [A::storage(), B::storage(), C::storage(), D::storage()]
            .into_iter()
            .flatten()
            .collect()
    }
    fn keyspaces() -> Vec<String> {
        [
            A::keyspaces(),
            B::keyspaces(),
            C::keyspaces(),
            D::keyspaces(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn build(context: &Context, _: &Values) -> Self {
        (
            A::build(context),
            B::build(context),
            C::build(context),
            D::build(context),
        )
    }
}
impl<A: Wiring, B: Wiring, C: Wiring, D: Wiring, E: Wiring> Wired for (A, B, C, D, E) {
    fn topics() -> Vec<SystemTopic> {
        [
            A::TOPICS.to_vec(),
            B::TOPICS.to_vec(),
            C::TOPICS.to_vec(),
            D::TOPICS.to_vec(),
            E::TOPICS.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn capabilities() -> Vec<Capability> {
        [
            A::CAPABILITIES.to_vec(),
            B::CAPABILITIES.to_vec(),
            C::CAPABILITIES.to_vec(),
            D::CAPABILITIES.to_vec(),
            E::CAPABILITIES.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn commands() -> Vec<CommandDependency> {
        [
            A::commands(),
            B::commands(),
            C::commands(),
            D::commands(),
            E::commands(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn required_topics() -> Vec<SystemTopic> {
        [
            A::required_topics(),
            B::required_topics(),
            C::required_topics(),
            D::required_topics(),
            E::required_topics(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn storage() -> Vec<StorageDescriptor> {
        [
            A::storage(),
            B::storage(),
            C::storage(),
            D::storage(),
            E::storage(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn keyspaces() -> Vec<String> {
        [
            A::keyspaces(),
            B::keyspaces(),
            C::keyspaces(),
            D::keyspaces(),
            E::keyspaces(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn build(context: &Context, _: &Values) -> Self {
        (
            A::build(context),
            B::build(context),
            C::build(context),
            D::build(context),
            E::build(context),
        )
    }
}
impl<A: Wiring, B: Wiring, C: Wiring, D: Wiring, E: Wiring, F: Wiring> Wired
    for (A, B, C, D, E, F)
{
    fn topics() -> Vec<SystemTopic> {
        [
            A::TOPICS.to_vec(),
            B::TOPICS.to_vec(),
            C::TOPICS.to_vec(),
            D::TOPICS.to_vec(),
            E::TOPICS.to_vec(),
            F::TOPICS.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn capabilities() -> Vec<Capability> {
        [
            A::CAPABILITIES.to_vec(),
            B::CAPABILITIES.to_vec(),
            C::CAPABILITIES.to_vec(),
            D::CAPABILITIES.to_vec(),
            E::CAPABILITIES.to_vec(),
            F::CAPABILITIES.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn commands() -> Vec<CommandDependency> {
        [
            A::commands(),
            B::commands(),
            C::commands(),
            D::commands(),
            E::commands(),
            F::commands(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn required_topics() -> Vec<SystemTopic> {
        [
            A::required_topics(),
            B::required_topics(),
            C::required_topics(),
            D::required_topics(),
            E::required_topics(),
            F::required_topics(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn storage() -> Vec<StorageDescriptor> {
        [
            A::storage(),
            B::storage(),
            C::storage(),
            D::storage(),
            E::storage(),
            F::storage(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn keyspaces() -> Vec<String> {
        [
            A::keyspaces(),
            B::keyspaces(),
            C::keyspaces(),
            D::keyspaces(),
            E::keyspaces(),
            F::keyspaces(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn build(context: &Context, _: &Values) -> Self {
        (
            A::build(context),
            B::build(context),
            C::build(context),
            D::build(context),
            E::build(context),
            F::build(context),
        )
    }
}
impl<A: Wiring, B: Wiring, C: Wiring, D: Wiring, E: Wiring, F: Wiring, G: Wiring> Wired
    for (A, B, C, D, E, F, G)
{
    fn topics() -> Vec<SystemTopic> {
        [
            A::TOPICS.to_vec(),
            B::TOPICS.to_vec(),
            C::TOPICS.to_vec(),
            D::TOPICS.to_vec(),
            E::TOPICS.to_vec(),
            F::TOPICS.to_vec(),
            G::TOPICS.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn capabilities() -> Vec<Capability> {
        [
            A::CAPABILITIES.to_vec(),
            B::CAPABILITIES.to_vec(),
            C::CAPABILITIES.to_vec(),
            D::CAPABILITIES.to_vec(),
            E::CAPABILITIES.to_vec(),
            F::CAPABILITIES.to_vec(),
            G::CAPABILITIES.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn commands() -> Vec<CommandDependency> {
        [
            A::commands(),
            B::commands(),
            C::commands(),
            D::commands(),
            E::commands(),
            F::commands(),
            G::commands(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn required_topics() -> Vec<SystemTopic> {
        [
            A::required_topics(),
            B::required_topics(),
            C::required_topics(),
            D::required_topics(),
            E::required_topics(),
            F::required_topics(),
            G::required_topics(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn storage() -> Vec<StorageDescriptor> {
        [
            A::storage(),
            B::storage(),
            C::storage(),
            D::storage(),
            E::storage(),
            F::storage(),
            G::storage(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn keyspaces() -> Vec<String> {
        [
            A::keyspaces(),
            B::keyspaces(),
            C::keyspaces(),
            D::keyspaces(),
            E::keyspaces(),
            F::keyspaces(),
            G::keyspaces(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn build(context: &Context, _: &Values) -> Self {
        (
            A::build(context),
            B::build(context),
            C::build(context),
            D::build(context),
            E::build(context),
            F::build(context),
            G::build(context),
        )
    }
}
impl<A: Wiring, B: Wiring, C: Wiring, D: Wiring, E: Wiring, F: Wiring, G: Wiring, H: Wiring> Wired
    for (A, B, C, D, E, F, G, H)
{
    fn topics() -> Vec<SystemTopic> {
        [
            A::TOPICS.to_vec(),
            B::TOPICS.to_vec(),
            C::TOPICS.to_vec(),
            D::TOPICS.to_vec(),
            E::TOPICS.to_vec(),
            F::TOPICS.to_vec(),
            G::TOPICS.to_vec(),
            H::TOPICS.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn capabilities() -> Vec<Capability> {
        [
            A::CAPABILITIES.to_vec(),
            B::CAPABILITIES.to_vec(),
            C::CAPABILITIES.to_vec(),
            D::CAPABILITIES.to_vec(),
            E::CAPABILITIES.to_vec(),
            F::CAPABILITIES.to_vec(),
            G::CAPABILITIES.to_vec(),
            H::CAPABILITIES.to_vec(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn commands() -> Vec<CommandDependency> {
        [
            A::commands(),
            B::commands(),
            C::commands(),
            D::commands(),
            E::commands(),
            F::commands(),
            G::commands(),
            H::commands(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn required_topics() -> Vec<SystemTopic> {
        [
            A::required_topics(),
            B::required_topics(),
            C::required_topics(),
            D::required_topics(),
            E::required_topics(),
            F::required_topics(),
            G::required_topics(),
            H::required_topics(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn storage() -> Vec<StorageDescriptor> {
        [
            A::storage(),
            B::storage(),
            C::storage(),
            D::storage(),
            E::storage(),
            F::storage(),
            G::storage(),
            H::storage(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn keyspaces() -> Vec<String> {
        [
            A::keyspaces(),
            B::keyspaces(),
            C::keyspaces(),
            D::keyspaces(),
            E::keyspaces(),
            F::keyspaces(),
            G::keyspaces(),
            H::keyspaces(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    fn build(context: &Context, _: &Values) -> Self {
        (
            A::build(context),
            B::build(context),
            C::build(context),
            D::build(context),
            E::build(context),
            F::build(context),
            G::build(context),
            H::build(context),
        )
    }
}
