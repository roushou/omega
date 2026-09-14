//! Compare active renderer attachments with this binary's embedded bundles.
use crate::ui::{Step, Ui};
use omega_proto::omega::{AttachRenderer, attach_renderer};
use std::time::Duration;

pub(super) struct RendererStatus;
pub(super) struct Snapshot {
    pub(super) active: Vec<AttachRenderer>,
    pub(super) placements: Vec<omega_proto::omega::PlacementAttachment>,
}
impl RendererStatus {
    pub(super) fn expected(attachment: &AttachRenderer) -> Option<String> {
        match attachment.scope.as_ref()? {
            attach_renderer::Scope::Placement(_) => Some(
                omega_omarchy::Renderer::VIEW
                    .build()
                    .fingerprint()
                    .to_owned(),
            ),
            attach_renderer::Scope::Unit(_) => {
                Some(omega_renderer::Desktop::build().fingerprint().to_owned())
            }
        }
    }
    pub(super) fn current(attachment: &AttachRenderer) -> bool {
        Self::expected(attachment).is_some_and(|expected| expected == attachment.build_fingerprint)
    }
    pub(super) fn show(
        attachments: &[AttachRenderer],
        placements: &[omega_proto::omega::PlacementAttachment],
        ui: &mut Ui,
    ) {
        for placement in placements {
            if !attachments.iter().any(|a| {
                a.scope.as_ref() == Some(&attach_renderer::Scope::Placement(placement.clone()))
            }) {
                ui.warn(format!(
                    "{}.{}#{}: no running renderer attached",
                    placement.unit, placement.surface, placement.placement
                ));
            }
        }
        if attachments.is_empty() {
            ui.warn("running renderer unverified: no active attachments reported by the daemon");
            return;
        }
        let mut current = 0;
        for attachment in attachments {
            if Self::current(attachment) {
                current += 1;
                continue;
            }
            let name = match &attachment.scope {
                Some(attach_renderer::Scope::Placement(p)) => {
                    format!("{}.{}#{}", p.unit, p.surface, p.placement)
                }
                Some(attach_renderer::Scope::Unit(unit)) => format!("{unit} desktop host"),
                None => "unknown renderer".to_owned(),
            };
            if attachment.build_fingerprint.is_empty() {
                ui.warn(format!(
                    "{name}: running renderer unverified (legacy or linked source)"
                ));
            } else {
                ui.warn(format!(
                    "{name}: running renderer is stale (build {})",
                    attachment.build_fingerprint
                ));
            }
        }
        if current > 0 {
            ui.step(
                Step::Checked,
                format!(
                    "{current}/{} running renderer attachments match this CLI",
                    attachments.len()
                ),
            );
        }
        if current != attachments.len() {
            ui.next("omega shell install; standalone hosts update when the local daemon restarts");
        }
    }

    pub(super) async fn read() -> anyhow::Result<Snapshot> {
        let status = tokio::time::timeout(
            Duration::from_secs(2),
            crate::operator::Operator::new().deployment(),
        )
        .await??;
        Ok(Snapshot {
            active: status.renderers,
            placements: status.renderer_placements,
        })
    }

    pub(super) fn activated(before: &[AttachRenderer], after: &[AttachRenderer]) -> bool {
        let placed: Vec<_> = after
            .iter()
            .filter(|a| matches!(a.scope, Some(attach_renderer::Scope::Placement(_))))
            .collect();
        !placed.is_empty()
            && placed.iter().all(|a| Self::current(a))
            && before
                .iter()
                .filter(|a| matches!(a.scope, Some(attach_renderer::Scope::Placement(_))))
                .all(|old| placed.iter().any(|new| old.scope == new.scope))
    }

    fn ready(before: &Snapshot, after: &Snapshot) -> bool {
        Self::activated(&before.active, &after.active)
            && before
                .placements
                .iter()
                .chain(&after.placements)
                .all(|placement| {
                    after.active.iter().any(|attachment| {
                        attachment.scope.as_ref()
                            == Some(&attach_renderer::Scope::Placement(placement.clone()))
                    })
                })
    }

    pub(super) async fn verify_activation(before: &Snapshot, ui: &mut Ui) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            let after = match Self::read().await {
                Ok(after) => after,
                Err(error) if tokio::time::Instant::now() >= deadline => {
                    return Err(error.context("renderer installed and shell restarted, but daemon status remained unavailable"));
                }
                Err(_) => continue,
            };
            if Self::ready(before, &after) {
                ui.step(
                    Step::Checked,
                    "running Omarchy renderer matches the installed build",
                );
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                Self::show(&after.active, &after.placements, ui);
                if before.placements.is_empty()
                    && after.placements.is_empty()
                    && before
                        .active
                        .iter()
                        .chain(&after.active)
                        .all(|a| !matches!(a.scope, Some(attach_renderer::Scope::Placement(_))))
                {
                    ui.warn("no active Omarchy placements were reported; installed files are verified, running code is not");
                    return Ok(());
                }
                anyhow::bail!(
                    "renderer installed and shell restarted, but activation could not be verified"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::omega::PlacementAttachment;
    struct Fixture;
    impl Fixture {
        fn attachment(unit: &str) -> AttachRenderer {
            AttachRenderer {
                scope: Some(attach_renderer::Scope::Placement(PlacementAttachment {
                    unit: unit.into(),
                    surface: "panel".into(),
                    placement: unit.into(),
                })),
                build_fingerprint: omega_omarchy::Renderer::VIEW.build().fingerprint().into(),
                ..Default::default()
            }
        }
    }
    #[test]
    fn activation_requires_current_builds_and_every_previous_placement() {
        let a = Fixture::attachment("audio");
        let b = Fixture::attachment("power");
        assert!(!RendererStatus::activated(&[], &[]));
        assert!(!RendererStatus::activated(
            &[a.clone(), b.clone()],
            std::slice::from_ref(&a)
        ));
        let mut stale = b.clone();
        stale.build_fingerprint = "a".repeat(64);
        assert!(!RendererStatus::activated(&[], &[a.clone(), stale]));
        let mut legacy = b.clone();
        legacy.build_fingerprint.clear();
        assert!(!RendererStatus::current(&legacy));
        assert!(RendererStatus::activated(&[a.clone(), b.clone()], &[b, a]));
    }
    #[test]
    fn disconnected_placements_cannot_disappear_from_activation_requirements() {
        let audio = Fixture::attachment("audio");
        let power = Fixture::attachment("power");
        let Some(attach_renderer::Scope::Placement(placement)) = power.scope.clone() else {
            unreachable!()
        };
        let before = Snapshot {
            active: vec![],
            placements: vec![placement.clone()],
        };
        let mut after = Snapshot {
            active: vec![audio],
            placements: vec![],
        };
        assert!(!RendererStatus::ready(&before, &after));
        after.active.push(power);
        assert!(RendererStatus::ready(&before, &after));
        after
            .placements
            .push(omega_proto::omega::PlacementAttachment {
                unit: "wifi".into(),
                surface: "panel".into(),
                placement: "wifi".into(),
            });
        assert!(!RendererStatus::ready(&before, &after));
    }
}
