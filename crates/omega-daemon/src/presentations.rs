//! Supervise one native presentation host per plugin with live standalone instances.
use crate::{Shutdown, hub::Hub, process::Signal};
use omega_host::{Directory, Layout, StageDir};
use omega_proto::UnitName;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::{Child, Command};

pub(crate) struct Hosts {
    hub: Hub,
    layout: Layout,
    socket: PathBuf,
    shutdown: Shutdown,
}
struct Host {
    stop: Shutdown,
    task: tokio::task::JoinHandle<()>,
}
impl Hosts {
    pub(crate) fn new(hub: Hub, layout: Layout, socket: PathBuf, shutdown: Shutdown) -> Self {
        Self {
            hub,
            layout,
            socket,
            shutdown,
        }
    }
    pub(crate) async fn run(self) {
        let mut hosts: BTreeMap<UnitName, Host> = BTreeMap::new();
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut installed = false;
        loop {
            tokio::select! { _ = self.shutdown.wait() => break, _ = tick.tick() => {} }
            let desired: std::collections::BTreeSet<_> = self
                .hub
                .view_snapshot()
                .into_iter()
                .filter(|view| {
                    matches!(
                        view.presentation.kind,
                        Some(
                            omega_proto::omega::presentation::Kind::Window(_)
                                | omega_proto::omega::presentation::Kind::Overlay(_)
                        )
                    )
                })
                .map(|view| view.surface.unit.clone())
                .collect();
            let removed: Vec<_> = hosts
                .keys()
                .filter(|unit| !desired.contains(*unit))
                .cloned()
                .collect();
            for unit in removed {
                if let Some(host) = hosts.remove(&unit) {
                    host.stop.trigger();
                    let _ = host.task.await;
                }
            }
            if !desired.is_empty() && !installed {
                let layout = self.layout.clone();
                match tokio::task::spawn_blocking(move || Self::install(&layout)).await {
                    Ok(Ok(())) => installed = true,
                    error => {
                        tracing::error!(?error, "cannot install standalone renderer");
                        continue;
                    }
                }
            }
            for unit in desired {
                hosts.entry(unit.clone()).or_insert_with(|| {
                    let stop = Shutdown::new();
                    let task = tokio::spawn(Self::supervise(
                        unit,
                        self.layout.clone(),
                        self.socket.clone(),
                        stop.clone(),
                        self.shutdown.clone(),
                    ));
                    Host { stop, task }
                });
            }
        }
        for host in hosts.values() {
            host.stop.trigger();
        }
        for (_, host) in hosts {
            let _ = host.task.await;
        }
    }
    fn install(layout: &Layout) -> std::io::Result<()> {
        let stage = StageDir::new(&layout.renderer_dir())?;
        let build = omega_renderer::Desktop::build();
        for asset in omega_renderer::Core::FILES
            .iter()
            .chain(omega_renderer::Desktop::FILES)
        {
            stage.write(asset.name, build.contents(asset).as_bytes())?;
        }
        stage.commit()
    }
    async fn supervise(
        unit: UnitName,
        layout: Layout,
        socket: PathBuf,
        stop: Shutdown,
        shutdown: Shutdown,
    ) {
        let mut delay = Duration::from_millis(250);
        loop {
            if stop.is_triggered() || shutdown.is_triggered() {
                return;
            }
            let started = tokio::time::Instant::now();
            match Self::spawn(&unit, &layout, &socket) {
                Ok(mut child) => {
                    tokio::select! {
                        result = child.wait() => tracing::warn!(%unit, ?result, "presentation host exited"),
                        _ = stop.wait() => { Self::terminate(&mut child).await; return; }
                        _ = shutdown.wait() => { Self::terminate(&mut child).await; return; }
                    }
                }
                Err(error) => tracing::error!(%unit, %error, "cannot start presentation host"),
            }
            if started.elapsed() > Duration::from_secs(30) {
                delay = Duration::from_millis(250);
            }
            tokio::select! { _ = stop.wait() => return, _ = shutdown.wait() => return, _ = tokio::time::sleep(delay) => {} }
            delay = (delay * 2).min(Duration::from_secs(30));
        }
    }
    fn spawn(unit: &UnitName, layout: &Layout, socket: &std::path::Path) -> std::io::Result<Child> {
        Directory::create_all(&layout.logs_dir())?;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(layout.renderer_log(unit))?;
        Command::new("quickshell")
            .arg("-p")
            .arg(layout.renderer_dir())
            .env("OMEGA_RENDERER_UNIT", unit.as_str())
            .env("OMEGA_RENDERER_SOCKET", socket)
            .env("QS_APP_ID", format!("org.omega.{}", unit.as_str()))
            .env("QS_NO_RELOAD_POPUP", "1")
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
    }
    async fn terminate(child: &mut Child) {
        if let Some(pid) = child.id() {
            let _ = Signal::terminate(pid as i32);
        }
        if tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
    }
}
