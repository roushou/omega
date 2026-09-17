//! Human-readable daemon health facts. Process state takes precedence over UI readiness.

use omega_host::Layout;
use omega_proto::PluginName;
use omega_proto::omega::{
    PluginHealth, PluginPhase, PluginReadiness, PluginStatus, PresentationState, RenderReadiness,
    SurfaceHealth,
};

use super::{Paint, Step, Ui};

impl Ui {
    pub(crate) fn plugin_health(
        &mut self,
        plugins: &[PluginStatus],
        health_reports: &[PluginHealth],
        layout: &Layout,
        detailed: bool,
    ) -> anyhow::Result<()> {
        let width = Self::width(plugins.iter().map(|plugin| plugin.plugin.as_str()));

        for plugin in plugins {
            let name = plugin.plugin.parse::<PluginName>()?;
            let health = health_reports
                .iter()
                .find(|health| health.plugin == plugin.plugin);
            let phase = PluginPhase::try_from(plugin.phase).unwrap_or(PluginPhase::Unspecified);
            let readiness = health.map(|health| health.readiness()).unwrap_or_default();
            let step = HealthDisplay::step(phase, readiness);
            let summary = HealthDisplay::summary(health, phase);

            self.step(
                step,
                format!("{}  {summary}", Self::column(&plugin.plugin, width)),
            );

            if detailed {
                self.detail(format!("Process: {}", HealthDisplay::phase(phase)));
                if let Some(health) = health {
                    self.detail(format!(
                        "Surfaces: {}",
                        if health.surfaces.is_empty() {
                            "none (background plugin)".into()
                        } else {
                            health.surfaces.join(", ")
                        }
                    ));
                }
            }

            if !plugin.detail.is_empty() {
                self.detail(&plugin.detail);
            }

            if plugin.restarts > 0 {
                self.detail(format!("Restarts: {}", plugin.restarts));
            }

            if matches!(
                phase,
                PluginPhase::Failed | PluginPhase::Restarting | PluginPhase::Stopped
            ) && plugin.last_exit_code >= 0
            {
                self.detail(format!("Last exit code: {}", plugin.last_exit_code));
            }

            if let Some(health) = health {
                for instance in &health.instances {
                    if detailed || instance.readiness != RenderReadiness::Ready as i32 {
                        HealthDisplay::instance(self, instance);
                    }
                }
            }

            if detailed || matches!(phase, PluginPhase::Failed | PluginPhase::Restarting) {
                self.detail(format!("Log: {}", Paint::path(layout.plugin_log(&name))));
            }
        }

        Ok(())
    }
}

struct HealthDisplay;

impl HealthDisplay {
    fn step(phase: PluginPhase, readiness: PluginReadiness) -> Step {
        match phase {
            PluginPhase::Starting => Step::Starting,
            PluginPhase::Restarting => Step::Restarting,
            PluginPhase::Failed => Step::Failed,
            PluginPhase::Stopped => Step::Stopped,
            PluginPhase::Unspecified => Step::Unknown,
            PluginPhase::Running => match readiness {
                PluginReadiness::Background => Step::Healthy,
                PluginReadiness::Failed => Step::Failed,
                PluginReadiness::Unplaced => Step::Unplaced,
                PluginReadiness::Waiting => Step::Waiting,
                PluginReadiness::Ready => Step::Ready,
                PluginReadiness::Unspecified => Step::Running,
            },
        }
    }

    fn phase(phase: PluginPhase) -> &'static str {
        match phase {
            PluginPhase::Starting => "starting (awaiting handshake)",
            PluginPhase::Running => "running",
            PluginPhase::Restarting => "restarting",
            PluginPhase::Failed => "failed",
            PluginPhase::Stopped => "stopped",
            PluginPhase::Unspecified => "unknown",
        }
    }

    fn summary(health: Option<&PluginHealth>, phase: PluginPhase) -> String {
        if phase != PluginPhase::Running {
            return Self::phase(phase).into();
        }

        let Some(health) = health else {
            return "UI readiness unavailable from this daemon".into();
        };

        match health.readiness() {
            PluginReadiness::Background => "background".into(),
            PluginReadiness::Failed => "surface render failed".into(),
            PluginReadiness::Unplaced => format!(
                "{} surface(s) available; no placement or open instance",
                health.surfaces.len()
            ),
            PluginReadiness::Unspecified => "UI readiness unknown".into(),
            PluginReadiness::Waiting | PluginReadiness::Ready => {
                let mut placements: Vec<_> = health
                    .instances
                    .iter()
                    .filter(|instance| !instance.placement.is_empty())
                    .map(|instance| instance.placement.as_str())
                    .collect();
                placements.sort_unstable();
                placements.dedup();

                if placements.is_empty() {
                    format!("{} transient instance(s)", health.instances.len())
                } else {
                    format!("placement {}", placements.join(", "))
                }
            }
        }
    }

    fn instance(ui: &mut Ui, instance: &SurfaceHealth) {
        let target = if instance.placement.is_empty() {
            let id = instance
                .instance
                .as_ref()
                .map(|instance| instance.id.as_str())
                .unwrap_or("pending");
            format!("{} · {id}", instance.surface)
        } else {
            format!("{} · placement {}", instance.surface, instance.placement)
        };

        let state = match instance.readiness() {
            RenderReadiness::Ready => "render ready",
            RenderReadiness::Failed => "render failed",
            RenderReadiness::Waiting => "waiting for first render",
            RenderReadiness::Unspecified if instance.instance.is_none() => {
                "awaiting instance construction"
            }
            RenderReadiness::Unspecified => "render readiness unavailable",
        };
        ui.detail(format!("{target}: {state}"));

        if !instance.render_error.is_empty() {
            ui.detail(&instance.render_error);
        }

        for topic in &instance.pending_topics {
            ui.detail(format!("Required reading not received: {topic}"));
        }

        if instance.instance.is_some() {
            ui.detail(format!(
                "Presentation: requested {}; observed {}",
                Self::presentation(instance.requested),
                Self::presentation(instance.observed)
            ));
        }
    }

    fn presentation(state: i32) -> &'static str {
        match PresentationState::try_from(state) {
            Ok(PresentationState::Visible) => "visible",
            Ok(PresentationState::Hidden) => "hidden",
            Ok(PresentationState::Closed) => "closed",
            Ok(PresentationState::Unspecified) | Err(_) => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture;

    impl Fixture {
        fn plugin(phase: PluginPhase) -> PluginStatus {
            PluginStatus {
                plugin: "network".into(),
                phase: phase as i32,
                last_exit_code: -1,
                ..Default::default()
            }
        }

        fn health(readiness: PluginReadiness) -> PluginHealth {
            PluginHealth {
                plugin: "network".into(),
                readiness: readiness as i32,
                surfaces: vec!["indicator".into()],
                ..Default::default()
            }
        }

        fn output(plugin: PluginStatus, health: PluginHealth, detailed: bool) -> String {
            let (mut ui, transcript) = Ui::recording();
            ui.plugin_health(&[plugin], &[health], &Layout::resolve(), detailed)
                .unwrap();
            assert!(transcript.out().is_empty());
            transcript.err()
        }
    }

    #[test]
    fn health_is_matched_by_plugin_name_instead_of_report_order() {
        let mut other = Fixture::health(PluginReadiness::Failed);
        other.plugin = "audio".into();
        let (mut ui, transcript) = Ui::recording();
        ui.plugin_health(
            &[Fixture::plugin(PluginPhase::Running)],
            &[other, Fixture::health(PluginReadiness::Ready)],
            &Layout::resolve(),
            false,
        )
        .unwrap();
        let output = transcript.err();
        assert!(!output.contains("Failed"), "{output}");
        assert!(output.contains("Ready"), "{output}");
    }

    #[test]
    fn process_failure_takes_precedence_and_explains_where_to_investigate() {
        let mut plugin = Fixture::plugin(PluginPhase::Restarting);
        plugin.last_exit_code = 1;
        plugin.restarts = 3;
        plugin.detail = "process exited".into();
        let output = Fixture::output(plugin, Fixture::health(PluginReadiness::Ready), false);
        assert!(output.contains("Restarting network"));
        assert!(output.contains("Last exit code: 1"));
        assert!(output.contains("Restarts: 3"));
        assert!(output.contains("process exited"));
        assert!(output.contains("network.log"));
        assert!(!output.contains("Ready network"));
    }

    #[test]
    fn detailed_waiting_status_names_readings_and_separates_visibility() {
        let mut health = Fixture::health(PluginReadiness::Waiting);
        health.instances.push(SurfaceHealth {
            surface: "indicator".into(),
            placement: "wifi".into(),
            instance: Some(omega_proto::omega::InstanceRef {
                id: "one".into(),
                incarnation: "session".into(),
            }),
            readiness: RenderReadiness::Waiting as i32,
            pending_topics: vec!["network".into()],
            render_error: String::new(),
            requested: PresentationState::Visible as i32,
            observed: PresentationState::Hidden as i32,
        });
        let output = Fixture::output(Fixture::plugin(PluginPhase::Running), health.clone(), true);
        assert!(output.contains("Waiting network"));
        assert!(output.contains("Process: running"));
        assert!(output.contains("placement wifi"));
        assert!(output.contains("Required reading not received: network"));
        assert!(output.contains("requested visible; observed hidden"));

        health.readiness = PluginReadiness::Ready as i32;
        health.instances[0].readiness = RenderReadiness::Ready as i32;
        health.instances[0].pending_topics.clear();
        let output = Fixture::output(Fixture::plugin(PluginPhase::Running), health, true);
        assert!(output.contains("Ready network"));
        assert!(output.contains("render ready"));
        assert!(output.contains("observed hidden"));
        assert!(!output.contains("Required reading"));
    }

    #[test]
    fn background_and_unplaced_are_informational_and_legacy_is_honest() {
        let running = Fixture::plugin(PluginPhase::Running);
        let output = Fixture::output(
            running.clone(),
            Fixture::health(PluginReadiness::Background),
            false,
        );
        assert!(output.contains("Healthy network  background"));
        let output = Fixture::output(
            running.clone(),
            Fixture::health(PluginReadiness::Unplaced),
            false,
        );
        assert!(output.contains("Unplaced network"));
        assert!(output.contains("no placement or open instance"));
        let output = Fixture::output(
            running,
            Fixture::health(PluginReadiness::Unspecified),
            false,
        );
        assert!(output.contains("UI readiness unknown"));
        assert!(!output.contains("Ready network"));
    }
    #[test]
    fn failed_render_status_includes_instance_diagnostic() {
        let mut health = Fixture::health(PluginReadiness::Failed);
        health.instances.push(SurfaceHealth {
            surface: "panel".into(),
            placement: "wifi".into(),
            readiness: RenderReadiness::Failed as i32,
            render_error: "binding capacity exceeded".into(),
            ..Default::default()
        });
        let output = Fixture::output(Fixture::plugin(PluginPhase::Running), health, true);
        assert!(output.contains("Failed network"));
        assert!(output.contains("Process: running"));
        assert!(output.contains("render failed"));
        assert!(output.contains("binding capacity exceeded"));
    }
}
