use crate::BrokerError;
use gio::{glib, prelude::*};
use omega_proto::{
    ApplicationId,
    omega::{Application, ApplicationsState, LaunchApp},
};
use tokio::sync::{mpsc, oneshot, watch};

type Answer<T> = oneshot::Sender<Result<T, BrokerError>>;
enum Request {
    Catalogue(Answer<ApplicationsState>),
    Launch(LaunchApp, Answer<()>),
}

/// GObjects never cross threads; only owned protocol values and completion channels do.
#[derive(Debug)]
pub(super) struct Worker {
    requests: mpsc::Sender<Request>,
    changes: watch::Receiver<u64>,
}
impl Worker {
    pub(super) fn start() -> Result<Self, BrokerError> {
        let (requests, inbox) = mpsc::channel(16);
        let (changes, watch) = watch::channel(0u64);
        std::thread::Builder::new()
            .name("omega-applications".into())
            .spawn(move || {
                let context = glib::MainContext::new();
                let result = context.with_thread_default(|| {
                    let main_loop = glib::MainLoop::new(Some(&context), false);
                    let stop = main_loop.clone();
                    let monitor = gio::AppInfoMonitor::get();
                    monitor.connect_changed(move |_| {
                        changes.send_modify(|revision| *revision = revision.wrapping_add(1));
                    });
                    context.spawn_local(async move {
                        Self::serve(inbox).await;
                        stop.quit();
                    });
                    main_loop.run();
                });
                if let Err(error) = result {
                    tracing::error!(%error, "application main context failed");
                }
            })?;
        Ok(Self {
            requests,
            changes: watch,
        })
    }
    pub(super) fn is_closed(&self) -> bool {
        self.requests.is_closed()
    }
    pub(super) async fn changed(&mut self) -> Result<(), BrokerError> {
        self.changes
            .changed()
            .await
            .map_err(BrokerError::unreadable)
    }
    pub(super) async fn catalogue(&self) -> Result<ApplicationsState, BrokerError> {
        let (answer, receive) = oneshot::channel();
        self.send(Request::Catalogue(answer))?;
        receive.await.map_err(BrokerError::unreadable)?
    }
    pub(super) async fn launch(&self, app: LaunchApp) -> Result<(), BrokerError> {
        let (answer, receive) = oneshot::channel();
        self.send(Request::Launch(app, answer))?;
        receive.await.map_err(BrokerError::unreadable)?
    }
    fn send(&self, request: Request) -> Result<(), BrokerError> {
        self.requests
            .try_send(request)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => BrokerError::Full,
                mpsc::error::TrySendError::Closed(_) => BrokerError::gone(),
            })
    }
    async fn serve(mut inbox: mpsc::Receiver<Request>) {
        while let Some(request) = inbox.recv().await {
            match request {
                Request::Catalogue(answer) => {
                    if !answer.is_closed() {
                        let _ = answer.send(Self::read());
                    }
                }
                Request::Launch(app, mut answer) => {
                    if answer.is_closed() {
                        continue;
                    }
                    // Dropping the caller cancels pending GIO work, never retries admission.
                    tokio::select! {
                        biased;
                        () = answer.closed() => {},
                        result = Self::activate(app) => { let _ = answer.send(result); },
                    }
                }
            }
        }
    }
    fn read() -> Result<ApplicationsState, BrokerError> {
        let mut applications = Vec::new();
        let mut bytes = 0;
        for app in gio::AppInfo::all() {
            if !app.should_show() {
                continue;
            }
            let Ok(desktop) = app.downcast::<gio::DesktopAppInfo>() else {
                continue;
            };
            if desktop.is_hidden() {
                continue;
            }
            let id = desktop
                .id()
                .ok_or_else(|| BrokerError::unreadable("desktop entry has no id"))?;
            let id = ApplicationId::try_from(id.to_string()).map_err(BrokerError::unreadable)?;
            let icon = match desktop.icon() {
                Some(icon) => {
                    if let Some(themed) = icon.downcast_ref::<gio::ThemedIcon>() {
                        themed
                            .names()
                            .first()
                            .map(ToString::to_string)
                            .unwrap_or_default()
                    } else if let Some(file) = icon.downcast_ref::<gio::FileIcon>() {
                        file.file()
                            .path()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                }
                None => String::new(),
            };
            let entry = Application {
                id: id.to_string(),
                name: desktop.display_name().to_string(),
                description: desktop
                    .description()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                generic_name: desktop
                    .generic_name()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                keywords: desktop
                    .keywords()
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect(),
                icon,
                terminal: desktop.boolean("Terminal"),
            };
            // Include protobuf field overhead so the catalogue fits one bounded state value.
            bytes += entry.id.len()
                + entry.name.len()
                + entry.description.len()
                + entry.generic_name.len()
                + entry.icon.len()
                + entry.keywords.iter().map(|s| s.len() + 8).sum::<usize>()
                + 64;
            if applications.len() >= 4096 || bytes > 512 * 1024 {
                return Err(BrokerError::TooLarge);
            }
            applications.push(entry);
        }
        applications.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(ApplicationsState { applications })
    }
    async fn activate(request: LaunchApp) -> Result<(), BrokerError> {
        omega_proto::omega::action::Kind::LaunchApp(request.clone())
            .validate()
            .map_err(BrokerError::unreadable)?;
        let app = gio::DesktopAppInfo::new(&request.desktop_id).ok_or_else(|| {
            BrokerError::unreadable(format!(
                "application {} is no longer installed",
                request.desktop_id
            ))
        })?;
        if app.is_hidden() {
            return Err(BrokerError::unreadable("application is hidden"));
        }
        if app.boolean("DBusActivatable") && !request.activation_token.is_empty() {
            return Err(BrokerError::Unsupported(
                "activation tokens for D-Bus applications require a native launch context".into(),
            ));
        }
        let context = gio::AppLaunchContext::new();
        context.unsetenv("DESKTOP_STARTUP_ID");
        context.unsetenv("XDG_ACTIVATION_TOKEN");
        if !request.activation_token.is_empty() {
            context.setenv("XDG_ACTIVATION_TOKEN", &request.activation_token);
            context.setenv("DESKTOP_STARTUP_ID", &request.activation_token);
        }
        let uris: Vec<_> = request.uris.iter().map(String::as_str).collect();
        app.launch_uris_future(&uris, Some(&context))
            .await
            .map_err(BrokerError::unreadable)
    }
}
