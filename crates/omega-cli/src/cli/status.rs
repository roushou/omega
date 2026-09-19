//! Inspect daemon activation, reconciliation, and plugin process status.

use std::time::Duration;

use anyhow::Context;
use omega_proto::PluginName;

use crate::ui::{Paint, Step, Ui};

/// Report plugin health and command host processes.
#[derive(Debug, clap::Args)]
pub struct StatusCmd {
    /// Inspect one plugin or command host.
    #[arg(value_name = "PLUGIN")]
    pub plugin_name: Option<PluginName>,

    /// Show CLI, daemon, renderer, and resolved config dependency versions.
    #[arg(long)]
    pub versions: bool,
    /// Print the daemon snapshot as JSON for scripts.
    #[arg(long, conflicts_with = "versions")]
    pub json: bool,
}

impl StatusCmd {
    /// How long to wait for the daemon's opening snapshot.
    const TIMEOUT: Duration = Duration::from_secs(2);

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        if self.versions {
            Self::versions(ui).await;
        }
        let mut status =
            tokio::time::timeout(Self::TIMEOUT, crate::operator::Operator::new().deployment())
                .await
                .context("the daemon did not report status in time")?
                .context("cannot read daemon status; ensure omega daemon is running")?;

        Self::select(&mut status, self.plugin_name.as_ref())?;

        if self.json {
            ui.line(serde_json::to_string(&status)?);
            return Ok(());
        }
        let layout = omega_host::Layout::resolve();
        let published = match omega_host::Generations::new(&layout).pin_current() {
            Ok(generation) => generation,
            Err(error) => {
                ui.warn(format!("published build unavailable: {error}"));
                None
            }
        };
        ui.deployment(
            &status,
            published
                .as_ref()
                .map(|generation| generation.id().as_str()),
        );
        if status.renderers.is_empty() && status.renderer_placements.is_empty() {
            ui.detail("No active renderer attachments.");
        } else {
            crate::renderer::RendererStatus::show(
                &status.renderers,
                &status.renderer_placements,
                ui,
            );
        }

        ui.blank();
        if status.plugins.is_empty() && status.command_hosts.is_empty() {
            ui.step(
                Step::Checked,
                "the daemon is running; no plugins or command hosts",
            );
        } else {
            ui.plugin_health(
                &status.plugins,
                &status.plugin_health,
                &layout,
                self.plugin_name.is_some(),
            )?;
        }
        ui.command_hosts(&status.command_hosts, self.plugin_name.is_some());
        Ok(())
    }

    fn select(
        status: &mut omega_proto::omega::DeploymentStatus,
        plugin_name: Option<&PluginName>,
    ) -> anyhow::Result<()> {
        use omega_proto::omega::attach_renderer;

        let Some(plugin_name) = plugin_name else {
            return Ok(());
        };

        anyhow::ensure!(
            status
                .plugins
                .iter()
                .any(|status| status.plugin == plugin_name.as_str())
                || status
                    .command_hosts
                    .iter()
                    .any(|host| host.id == plugin_name.as_str()),
            "unknown plugin or command host {plugin_name}"
        );
        status
            .command_hosts
            .retain(|host| host.id == plugin_name.as_str());
        status
            .plugins
            .retain(|status| status.plugin == plugin_name.as_str());
        status
            .plugin_health
            .retain(|status| status.plugin == plugin_name.as_str());
        status
            .renderer_placements
            .retain(|placement| placement.plugin == plugin_name.as_str());
        status.renderers.retain(|renderer| match &renderer.scope {
            Some(attach_renderer::Scope::Plugin(name)) => name == plugin_name.as_str(),
            Some(attach_renderer::Scope::Placement(placement)) => {
                placement.plugin == plugin_name.as_str()
            }
            None => false,
        });

        Ok(())
    }

    async fn versions(ui: &mut Ui) {
        let version = env!("CARGO_PKG_VERSION");
        ui.step(Step::Checking, format!("CLI {version}"));
        match std::env::current_exe() {
            Ok(path) => ui.detail(format!("executable: {}", Paint::path(path))),
            Err(error) => ui.warn(format!("CLI executable path unavailable: {error}")),
        }
        match tokio::time::timeout(
            Self::TIMEOUT,
            crate::operator::Operator::new().daemon_version(),
        )
        .await
        {
            Ok(Ok(daemon)) if daemon == version => {
                ui.step(Step::Checked, format!("daemon {daemon}"))
            }
            Ok(Ok(daemon)) => ui.warn(format!("daemon {daemon}; CLI {version}")),
            Ok(Err(error)) => ui.warn(format!("daemon version unavailable: {error}")),
            Err(_) => ui.warn("daemon version request timed out"),
        }
        if let Some(shell) = omega_omarchy::HostShell::detect() {
            for renderer in omega_omarchy::Renderer::ALL {
                use omega_omarchy::Installed;
                match renderer.installed(&shell.plugins()) {
                    Installed::Current => ui.step(
                        Step::Checked,
                        format!(
                            "{} {} installed files match this CLI",
                            renderer.id,
                            omega_omarchy::Renderer::VERSION
                        ),
                    ),
                    Installed::Missing => ui.warn(format!("{} is not installed", renderer.id)),
                    Installed::Linked(path) => ui.step(
                        Step::Linked,
                        format!("{} from {}", renderer.id, Paint::path(path)),
                    ),
                    installed @ Installed::Stale { .. } => ui.warn(format!(
                        "{}: {}",
                        renderer.id,
                        installed
                            .difference()
                            .expect("stale renderer has a difference")
                    )),
                }
            }
        } else {
            ui.detail("No supported shell detected; installed renderer unavailable.");
        }
        let layout = omega_host::Layout::resolve();
        if !layout.workspace_manifest().exists() {
            ui.detail("No configuration workspace.");
            return;
        }
        match tokio::time::timeout(
            Duration::from_secs(10),
            omega_host::cargo::Cargo::new(&layout.config).metadata(
                omega_host::cargo::MetadataRequest::new()
                    .resolution(omega_host::cargo::Resolution::OfflineLocked),
            ),
        )
        .await
        {
            Ok(Ok(metadata)) => {
                let mut packages = metadata.packages;
                ui.step(Step::Checking, "resolved configuration dependencies");
                packages.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
                for package in packages
                    .into_iter()
                    .filter(|p| p.name.starts_with("omega-"))
                {
                    let source = match package.source {
                        Some(source) => source.to_string(),
                        None => {
                            format!("path {}", Paint::path(package.manifest_path.as_std_path()))
                        }
                    };
                    ui.step(
                        Step::Checking,
                        format!("{} {} — {source}", package.name, package.version),
                    );
                }
            }
            Ok(Err(error)) => ui.warn(format!("config dependency versions unavailable: {error}")),
            Err(_) => ui.warn("config dependency version lookup timed out"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega_proto::omega::{
        AttachRenderer, DeploymentStatus, PluginHealth, PluginStatus, attach_renderer,
    };

    #[test]
    fn selecting_a_command_host_preserves_process_facts_in_json() {
        use omega_proto::omega::{CommandHostStatus, CommandProcessStatus};

        let mut status = DeploymentStatus {
            command_hosts: vec![CommandHostStatus {
                id: "audio-commands".into(),
                lifetime: "persistent".into(),
                processes: vec![CommandProcessStatus {
                    id: 7,
                    phase: "running".into(),
                }],
                ..Default::default()
            }],
            plugins: vec![PluginStatus {
                plugin: "audio".into(),
                ..Default::default()
            }],
            plugin_health: vec![PluginHealth {
                plugin: "audio".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut plugin_only = status.clone();
        StatusCmd::select(&mut plugin_only, Some(&"audio".parse().unwrap())).unwrap();
        assert!(plugin_only.command_hosts.is_empty());

        StatusCmd::select(&mut status, Some(&"audio-commands".parse().unwrap())).unwrap();
        assert!(status.plugins.is_empty());
        assert!(status.plugin_health.is_empty());
        assert_eq!(status.command_hosts[0].processes[0].id, 7);
        let json = serde_json::to_string(&status).unwrap();
        let decoded: DeploymentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, status);
        assert!(json.contains("commandHosts"));
        assert!(json.contains("running"));
    }

    #[test]
    fn selecting_a_plugin_filters_json_facts_and_rejects_unknown_names() {
        let mut status = DeploymentStatus {
            accepted_generation: "retained".into(),
            ..Default::default()
        };
        for name in ["audio", "network"] {
            status.plugins.push(PluginStatus {
                plugin: name.into(),
                ..Default::default()
            });
            status.plugin_health.push(PluginHealth {
                plugin: name.into(),
                ..Default::default()
            });
            status.renderers.push(AttachRenderer {
                scope: Some(attach_renderer::Scope::Plugin(name.into())),
                ..Default::default()
            });
        }

        assert!(
            StatusCmd::select(
                &mut status,
                Some(&"missing".parse::<omega_proto::PluginName>().unwrap())
            )
            .is_err()
        );
        assert_eq!(status.plugins.len(), 2);
        StatusCmd::select(
            &mut status,
            Some(&"network".parse::<omega_proto::PluginName>().unwrap()),
        )
        .unwrap();
        assert_eq!(status.plugins.len(), 1);
        assert_eq!(status.plugin_health.len(), 1);
        assert_eq!(status.renderers.len(), 1);
        assert_eq!(status.plugins[0].plugin, "network");
        assert_eq!(status.accepted_generation, "retained");
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("audio"));
        assert!(json.contains("network"));
    }
}
