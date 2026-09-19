//! Configured executable command providers and their admitted executions.
mod invocations;
mod processes;
mod startup;

use crate::Shutdown;
use crate::refusal::RefusableResult;
use omega_proto::host::{HostId, HostPolicy, InvocationId, Lifetime, ProcessId};
use omega_proto::omega::{CallCommand, CommandHostConfig, invoke};
use omega_proto::{CommandAnswer, CommandId, Manifest, Refusal, Socket, Values};
use processes::{Process, Processes};
use startup::Startup;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{Semaphore, mpsc, oneshot};

#[derive(Debug, Clone)]
pub(crate) struct Hosts {
    inner: Arc<Inner>,
}
#[derive(Debug)]
struct Inner {
    environment: Mutex<Option<(Socket, Shutdown)>>,
    providers: Mutex<BTreeMap<HostId, Arc<Provider>>>,
    processes: Arc<Processes>,
    invocations: Arc<invocations::Invocations>,
    bytes: Arc<Semaphore>,
    process_slots: Arc<Semaphore>,
}
#[derive(Debug)]
struct Provider {
    manifest: Arc<Manifest>,
    policy: HostPolicy,
    settings: Values,
    generation: omega_host::Generation,
    queue: mpsc::Sender<Job>,
    admission: Arc<Semaphore>,
    retired: Shutdown,
    startup: Arc<Mutex<Startup>>,
}

#[derive(Debug)]
struct Job {
    execution: invocations::Execution,
    admitted: tokio::time::Instant,
    call: CallCommand,
    answer: oneshot::Sender<Result<CommandAnswer, Refusal>>,
    _bytes: tokio::sync::OwnedSemaphorePermit,
    _admission: tokio::sync::OwnedSemaphorePermit,
    endpoint: omega_proto::omega::CommandEndpoint,
}

impl Default for Hosts {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                environment: Mutex::new(None),
                providers: Mutex::new(BTreeMap::new()),
                processes: Arc::new(Processes::default()),
                invocations: Arc::new(invocations::Invocations::default()),
                bytes: Arc::new(Semaphore::new(8 * 1024 * 1024)),
                process_slots: Arc::new(Semaphore::new(32)),
            }),
        }
    }
}

impl Hosts {
    pub(crate) fn inspect(&self) -> Vec<omega_proto::omega::CommandHostStatus> {
        self.inner
            .providers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|provider| {
                let now = tokio::time::Instant::now();
                let processes = self.inner.processes.inspect(&provider.policy.id);
                let activity = self.inner.invocations.activity(&provider.policy.id, now);
                let startup = provider.startup.lock().unwrap_or_else(|e| e.into_inner());
                omega_proto::omega::CommandHostStatus {
                    id: provider.policy.id.to_string(),
                    lifetime: match provider.policy.lifetime {
                        Lifetime::OneShot => "one-shot",
                        Lifetime::Persistent(_) => "persistent",
                    }
                    .into(),
                    phase: startup.phase(&processes, provider.policy.lifetime, now) as i32,
                    start: match provider.policy.lifetime {
                        Lifetime::Persistent(omega_proto::host::StartPolicy::Eager) => {
                            omega_proto::omega::HostStart::Eager
                        }
                        _ => omega_proto::omega::HostStart::OnDemand,
                    } as i32,
                    retry_after_ms: startup.retry_after(now),
                    startup_error: startup.error().unwrap_or_default().into(),
                    processes,
                    concurrency: provider.policy.execution.concurrency.get() as u32,
                    queue_capacity: provider.policy.execution.queue_capacity as u32,
                    queued_calls: activity.queued,
                    active_calls: activity.active,
                    recent_failures: activity.failures,
                }
            })
            .collect()
    }

    pub(crate) fn executions(
        &self,
        host: &str,
        command: &str,
    ) -> Vec<omega_proto::omega::CommandExecution> {
        match host.parse() {
            Ok(host) => self
                .inner
                .invocations
                .inspect(&host, command, tokio::time::Instant::now()),
            Err(_) => Vec::new(),
        }
    }

    pub(crate) fn all_stopped(&self) -> bool {
        self.inner.processes.is_empty()
    }

    pub(crate) fn environment(&self, socket: Socket, shutdown: Shutdown) {
        *self
            .inner
            .environment
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((socket, shutdown));
    }
    pub(crate) fn configure(
        &self,
        manifests: &crate::manifest::ManifestStore,
        configs: &[CommandHostConfig],
        generation: &omega_host::Generation,
    ) -> Result<(), Refusal> {
        let shutdown = self
            .inner
            .environment
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(_, shutdown)| shutdown.clone())
            .ok_or_else(|| {
                Refusal::precondition("command process environment is not initialized")
            })?;
        let mut next = BTreeMap::new();
        let mut workers = Vec::new();
        let mut providers = self
            .inner
            .providers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for config in configs {
            let policy = HostPolicy::try_from(config)
                .map_err(|error| Refusal::invalid(error.to_string()))?;
            if next.contains_key(&policy.id) {
                return Err(Refusal::invalid("duplicate command host configuration"));
            }
            let name = config
                .id
                .parse()
                .map_err(|error: omega_proto::IdentError| Refusal::invalid(error.to_string()))?;
            let manifest = manifests
                .get(&name)
                .ok_or_else(|| Refusal::invalid("unknown command host"))?
                .manifest
                .clone();
            if manifest.host_kind != omega_proto::omega::HostKind::Commands as i32 {
                return Err(Refusal::invalid("command host is not a command executable"));
            }
            let settings = Values::from_map(config.settings.clone());
            if let Some(old) = providers.get(&policy.id)
                && old.generation.id() == generation.id()
                && old.policy == policy
                && old.settings == settings
            {
                next.insert(policy.id.clone(), old.clone());
                continue;
            }
            let (queue, requests) =
                mpsc::channel(policy.execution.queue_capacity + policy.execution.concurrency.get());
            let provider = Arc::new(Provider {
                manifest: Arc::new(manifest),
                settings,
                admission: Arc::new(Semaphore::new(
                    policy.execution.queue_capacity + policy.execution.concurrency.get(),
                )),
                retired: Shutdown::new(),
                startup: Arc::new(Mutex::new(Startup::default())),
                policy,
                generation: generation.clone(),
                queue,
            });
            next.insert(provider.policy.id.clone(), provider.clone());
            workers.push((provider, requests));
        }
        for (id, old) in providers.iter() {
            if next.get(id).is_none_or(|new| !Arc::ptr_eq(old, new)) {
                old.retired.trigger();
            }
        }
        *providers = next;
        drop(providers);
        for (provider, requests) in workers {
            let hosts = self.clone();
            let shutdown = shutdown.clone();
            let task = format!("command host {}", provider.policy.id);
            tokio::spawn(async move {
                shutdown
                    .supervise(task, hosts.serve_provider(provider, requests))
                    .await;
            });
        }
        Ok(())
    }

    pub(crate) fn configured(&self, host: &str) -> bool {
        host.parse::<HostId>().ok().is_some_and(|id| {
            self.inner
                .providers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(&id)
        })
    }

    pub(crate) async fn invoke(
        &self,
        host: &str,
        command: CommandId,
        args: Vec<omega_proto::omega::Value>,
        signature: &[u8],
        typed: bool,
    ) -> Result<CommandAnswer, Refusal> {
        let id: HostId = host
            .parse()
            .map_err(|error: omega_proto::IdentError| Refusal::invalid(error.to_string()))?;
        let provider = self
            .inner
            .providers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
            .ok_or_else(|| Refusal::unavailable("command provider is not configured"))?;
        let endpoint = provider
            .manifest
            .commands
            .iter()
            .find(|endpoint| endpoint.id == command.as_str())
            .cloned()
            .ok_or_else(|| Refusal::invalid("command not exported by provider"))?;
        if endpoint.signature() != signature {
            return Err(Refusal::precondition("command contract changed"));
        }
        if typed {
            endpoint
                .accepts_arguments(&args)
                .map_err(|error| Refusal::invalid(error.to_string()))?;
        }
        let admission = provider
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| Refusal::exhausted("command provider capacity exhausted"))?;
        let execution =
            self.inner
                .invocations
                .admit(id, command.clone(), tokio::time::Instant::now())?;
        let call = CallCommand {
            invocation_id: execution.id.get(),
            command: command.to_string(),
            args,
        };
        use prost::Message;
        let bytes = u32::try_from(call.encoded_len())
            .map_err(|_| Refusal::exhausted("command payload too large"))?;
        let permit = self
            .inner
            .bytes
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| Refusal::exhausted("command payload capacity exhausted"))?;
        let (answer, receive) = oneshot::channel();
        provider
            .queue
            .try_send(Job {
                execution,
                admitted: tokio::time::Instant::now(),
                call,
                answer,
                _bytes: permit,
                _admission: admission,
                endpoint,
            })
            .map_err(|_| Refusal::exhausted("command queue is full"))?;
        receive
            .await
            .map_err(|_| Refusal::unavailable("command provider stopped"))?
    }

    async fn serve_provider(&self, provider: Arc<Provider>, mut requests: mpsc::Receiver<Job>) {
        let Some((socket, shutdown)) = self
            .inner
            .environment
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return;
        };
        let mut running = tokio::task::JoinSet::new();
        let persistent = Arc::new(tokio::sync::Mutex::new(None));
        let mut retiring = false;
        let mut input_closed = false;
        let mut maintenance = tokio::time::interval(std::time::Duration::from_millis(500));
        let mut queued = std::collections::VecDeque::<Job>::new();
        if matches!(
            provider.policy.lifetime,
            Lifetime::Persistent(omega_proto::host::StartPolicy::Eager)
        ) {
            match self.start(&provider, &socket, &shutdown, None) {
                Ok(process) => {
                    *persistent.lock().await = Some(process);
                }
                Err(error) => {
                    tracing::error!(host = %provider.policy.id, %error, "command host startup failed")
                }
            }
        }
        loop {
            while running.len() < provider.policy.execution.concurrency.get() {
                let Some(mut job) = queued.pop_front() else {
                    break;
                };
                if job.answer.is_closed() {
                    continue;
                }
                if job.admitted.elapsed() >= provider.policy.execution.queue_timeout {
                    job.execution.finish(
                        Err(omega_proto::omega::ErrorCode::DeadlineExceeded),
                        tokio::time::Instant::now(),
                    );
                    let _ = job
                        .answer
                        .send(Err(Refusal::deadline("command expired before execution")));
                    continue;
                }
                let hosts = self.clone();
                let provider = provider.clone();
                let socket = socket.clone();
                let shutdown = shutdown.clone();
                let persistent = persistent.clone();
                running.spawn(async move {
                    job.execution.starting(tokio::time::Instant::now());
                    let result = hosts
                        .execute(&provider, &socket, &shutdown, &persistent, &job)
                        .await
                        .and_then(|answer| {
                            job.endpoint
                                .accepts_answer(&answer)
                                .map_err(|error| Refusal::precondition(error.to_string()))?;
                            Ok(answer)
                        });
                    job.execution.finish(
                        result.as_ref().map(|_| ()).map_err(|error| error.code),
                        tokio::time::Instant::now(),
                    );
                    let _ = job.answer.send(result);
                });
            }
            if input_closed && running.is_empty() && queued.is_empty() {
                break;
            }
            let deadline = queued
                .iter()
                .map(|job| job.admitted + provider.policy.execution.queue_timeout)
                .min();
            tokio::select! {
                _ = shutdown.wait() => break,
                _ = maintenance.tick(), if !retiring && matches!(provider.policy.lifetime, Lifetime::Persistent(omega_proto::host::StartPolicy::Eager)) => {
                    let mut held = persistent.lock().await;
                    if held.as_ref().is_none_or(|process| !self.inner.processes.contains(process.id)) {
                        match self.start(&provider, &socket, &shutdown, None) {
                            Ok(process) => *held = Some(process),
                            Err(error) => tracing::debug!(host = %provider.policy.id, %error, "eager command host awaiting restart"),
                        }
                    }
                },
                _ = provider.retired.wait(), if !retiring => { retiring = true; requests.close(); },
                Some(result) = running.join_next(), if !running.is_empty() => {
                    if let Err(error) = result { tracing::error!(%error, host = %provider.policy.id, "command execution task failed"); shutdown.trigger(); break; }
                },
                job = requests.recv(), if !input_closed => {
                    match job { Some(job) => queued.push_back(job), None => input_closed = true }
                }
                _ = async { match deadline { Some(deadline) => tokio::time::sleep_until(deadline).await, None => std::future::pending().await } } => {
                    let now = tokio::time::Instant::now();
                    let mut retained = std::collections::VecDeque::new();
                    while let Some(mut job) = queued.pop_front() {
                        if now >= job.admitted + provider.policy.execution.queue_timeout { job.execution.finish(Err(omega_proto::omega::ErrorCode::DeadlineExceeded), now); let _ = job.answer.send(Err(Refusal::deadline("command expired before execution"))); }
                        else { retained.push_back(job); }
                    }
                    queued = retained;
                }
            }
        }
        while running.join_next().await.is_some() {}
        if let Some(process) = persistent.lock().await.take() {
            process.stop.trigger();
            self.inner.processes.reaped(process.id).await;
        }
    }

    async fn execute(
        &self,
        provider: &Provider,
        socket: &Socket,
        shutdown: &Shutdown,
        persistent: &tokio::sync::Mutex<Option<Process>>,
        job: &Job,
    ) -> Result<CommandAnswer, Refusal> {
        let process = match provider.policy.lifetime {
            Lifetime::OneShot => self.start(provider, socket, shutdown, Some(job.execution.id))?,
            Lifetime::Persistent(_) => {
                let mut held = persistent.lock().await;
                if held
                    .as_ref()
                    .is_none_or(|process| process.stop.is_triggered())
                {
                    if let Some(previous) = held.take() {
                        self.inner.processes.reaped(previous.id).await;
                    }
                    *held = Some(self.start(provider, socket, shutdown, None)?);
                }
                held.as_ref()
                    .cloned()
                    .ok_or_else(|| Refusal::unavailable("command host did not start"))?
            }
        };
        let ready = tokio::time::timeout(
            provider.policy.execution.startup_timeout,
            self.inner.processes.ready(process.id),
        )
        .await;
        let session = match ready {
            Ok(Ok(session)) => session,
            Ok(Err(error)) => {
                process.stop.trigger();
                self.inner.processes.reaped(process.id).await;
                return Err(error);
            }
            Err(_) => {
                self.inner
                    .processes
                    .fail_startup(process.id, "handshake timed out");
                process.stop.trigger();
                self.inner.processes.reaped(process.id).await;
                return Err(Refusal::unavailable("command host startup timed out"));
            }
        };
        let name = provider
            .policy
            .id
            .as_str()
            .parse()
            .map_err(|error: omega_proto::IdentError| Refusal::invalid(error.to_string()))?;
        job.execution.dispatched(process.id);
        let result = tokio::time::timeout(
            provider.policy.execution.execution_timeout,
            session.request(
                &name,
                invoke::Op::CallCommand(job.call.clone()),
                provider.policy.execution.execution_timeout,
            ),
        )
        .await;
        let answer = match result {
            Ok(Err(
                crate::plugins::RequestError::Timeout(_) | crate::plugins::RequestError::Absent(_),
            )) => Err(Refusal::new(
                omega_proto::omega::ErrorCode::OutcomeUnknown,
                "command connection ended or timed out; effects may have executed",
            )),
            Ok(result) => result.or_refuse().and_then(CommandAnswer::try_from),
            Err(_) => Err(Refusal::new(
                omega_proto::omega::ErrorCode::OutcomeUnknown,
                "command timed out; effects may have executed",
            )),
        };
        if matches!(provider.policy.lifetime, Lifetime::OneShot)
            || answer.as_ref().is_err_and(|error| {
                matches!(
                    error.code,
                    omega_proto::omega::ErrorCode::OutcomeUnknown
                        | omega_proto::omega::ErrorCode::DeadlineExceeded
                        | omega_proto::omega::ErrorCode::Unavailable
                )
            })
        {
            if matches!(provider.policy.lifetime, Lifetime::OneShot) && answer.is_ok() {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    self.inner.processes.reaped(process.id),
                )
                .await;
            }
            process.stop.trigger();
            self.inner.processes.reaped(process.id).await;
        }
        answer
    }

    fn start(
        &self,
        provider: &Provider,
        socket: &Socket,
        shutdown: &Shutdown,
        invocation: Option<InvocationId>,
    ) -> Result<Process, Refusal> {
        if shutdown.is_triggered() {
            return Err(Refusal::unavailable("daemon is stopping"));
        }
        if matches!(provider.policy.lifetime, Lifetime::Persistent(_)) {
            provider
                .startup
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reserve(tokio::time::Instant::now())?;
        }
        let name = provider
            .manifest
            .name
            .parse()
            .map_err(|error: omega_proto::IdentError| Refusal::invalid(error.to_string()))?;
        let log = crate::supervisor::PluginLog::at(provider.generation.layout().plugin_log(&name));
        let process_slot = self
            .inner
            .process_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Refusal::exhausted("command process capacity exhausted"))
            .inspect_err(|error| {
                provider
                    .startup
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .failed(&error.message)
            })?;
        let process = self
            .inner
            .processes
            .prepare(
                provider.policy.id.clone(),
                invocation,
                provider.manifest.clone(),
                provider.settings.clone(),
                provider.startup.clone(),
            )
            .inspect_err(|error| {
                provider
                    .startup
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .failed(&error.message)
            })?;
        let mut child = match crate::process::ManagedChild::spawn(
            &provider.generation.layout().state_plugin_program(&name),
            socket,
            process.identity.token(),
            Some(&provider.generation),
            Some(&log),
        ) {
            Ok(child) => child,
            Err(error) => {
                provider
                    .startup
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .failed(&error.to_string());
                self.inner.processes.remove(process.id);
                return Err(Refusal::unavailable(error.to_string()));
            }
        };
        if let Some(pid) = child.id() {
            self.inner.processes.bind(process.id, pid as i32);
        }
        let processes = self.inner.processes.clone();
        let watched = process.clone();
        let shutdown = shutdown.clone();
        let generation = provider.generation.clone();
        let startup_timeout = provider.policy.execution.startup_timeout;
        tokio::spawn(async move {
            let _generation = generation;
            let _process_slot = process_slot;
            let ready = tokio::select! {
                ready = tokio::time::timeout(startup_timeout, processes.ready(watched.id)) => {
                    match ready {
                        Ok(Ok(_)) => true,
                        Ok(Err(error)) => {
                            if !shutdown.is_triggered() {
                                processes.fail_startup(watched.id, &error.message);
                            }
                            false
                        }
                        Err(_) => {
                            processes.fail_startup(watched.id, "handshake timed out");
                            false
                        }
                    }
                },
                _ = shutdown.wait() => false,
                result = child.wait() => {
                    if !shutdown.is_triggered() {
                        let message = match &result {
                            Ok(status) => format!("process exited before handshake: {status}"),
                            Err(error) => format!("could not wait for startup: {error}"),
                        };
                        processes.fail_startup(watched.id, &message);
                    }
                    if let Err(error) = result { tracing::error!(%error, "command process wait failed"); shutdown.trigger(); return; }
                    processes.remove(watched.id);
                    return;
                }
            };
            if !ready {
                watched.stop.trigger();
            }
            let result = tokio::select! {
                result = child.wait() => result,
                _ = watched.stop.wait() => crate::process::ManagedChild::stop(&mut child).await,
                _ = shutdown.wait() => crate::process::ManagedChild::stop(&mut child).await,
            };
            if let Err(error) = result {
                tracing::error!(process = %watched.id, %error, "could not reap command process");
                shutdown.trigger();
                return;
            }
            processes.remove(watched.id);
        });
        Ok(process)
    }

    pub(crate) fn authenticate(
        &self,
        pid: i32,
        token: &str,
        hash: &str,
    ) -> Result<Option<crate::session::admission::Peer>, Refusal> {
        self.inner
            .processes
            .authenticate(pid, token, hash)?
            .map(|process| {
                crate::session::admission::Peer::host(
                    process.id,
                    process.host,
                    process.invocation,
                    process.manifest,
                    process.settings,
                )
            })
            .transpose()
    }
    pub(crate) fn connect(
        &self,
        id: ProcessId,
        sender: mpsc::Sender<crate::plugins::session::Request>,
    ) -> Result<processes::Connection, Refusal> {
        self.inner.processes.connect(id, sender)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ManifestStore;
    use crate::process::session::Request;
    use omega_host::{Generation, Generations, Layout, TempPath};
    use omega_proto::host::{ExecutionPolicy, StartPolicy};
    use omega_proto::omega::{
        CommandEndpoint, CommandHostPhase, CommandType, ErrorCode, HostKind, command_type,
    };
    use std::{
        future::Future, num::NonZeroUsize, os::unix::fs::PermissionsExt, path::PathBuf,
        time::Duration,
    };
    use tokio::task::JoinHandle;

    type Call = JoinHandle<Result<CommandAnswer, Refusal>>;

    /// Real supervised children and generation leases, with explicit session replies.
    /// Authentication and wire transport are covered by process/session and CLI tests.
    struct Fixture {
        root: PathBuf,
        layout: Layout,
        hosts: Hosts,
        shutdown: Shutdown,
        manifests: ManifestStore,
        endpoint: CommandEndpoint,
    }

    struct Peer {
        _connection: processes::Connection,
        requests: mpsc::Receiver<Request>,
    }

    impl Peer {
        async fn request(&mut self) -> Request {
            Fixture::within(self.requests.recv())
                .await
                .expect("host session ended")
        }

        fn complete(request: Request) {
            assert!(matches!(request.op, invoke::Op::CallCommand(_)));
            request
                .answer
                .send(Ok(CommandAnswer::Acknowledged.into_outcome()))
                .unwrap();
        }
    }

    impl Fixture {
        const PROGRAM: &'static [u8] = b"#!/bin/sh\nexec /bin/sleep 300\n";

        fn new() -> Self {
            let root =
                TempPath::sibling(&std::env::temp_dir().join("omega-host-lifecycle"), "test");
            let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
            let hosts = Hosts::default();
            let shutdown = Shutdown::new();
            hosts.environment(Socket::at(root.join("control.sock")), shutdown.clone());
            let endpoint = CommandEndpoint {
                id: "test.run".into(),
                input: Some(CommandType::of(command_type::Kind::Unit)),
                output: Some(CommandType::of(command_type::Kind::Unit)),
                ..Default::default()
            };
            let mut manifest =
                Manifest::new(&"worker".parse().unwrap(), "1").serving([endpoint.clone()]);
            manifest.host_kind = HostKind::Commands as i32;
            Self {
                root,
                layout,
                hosts,
                shutdown,
                manifests: ManifestStore::from_manifests([manifest]),
                endpoint,
            }
        }

        fn generation(&self, program: Option<&[u8]>) -> Generation {
            let generations = Generations::new(&self.layout);
            let stage = generations.stage().unwrap();
            if let Some(program) = program {
                let name = "worker".parse().unwrap();
                let layout = Layout::at(
                    &self.layout.config,
                    stage.files().path(),
                    &self.layout.cache,
                );
                stage
                    .files()
                    .write(layout.plugin_program_rel(&name), program)
                    .unwrap();
                std::fs::set_permissions(
                    layout.state_plugin_program(&name),
                    std::fs::Permissions::from_mode(0o755),
                )
                .unwrap();
            }
            stage.commit().unwrap();
            generations.pin_current().unwrap().unwrap()
        }

        fn configure(
            &self,
            generation: &Generation,
            lifetime: Lifetime,
            concurrency: usize,
            queue: usize,
        ) {
            let mut execution = ExecutionPolicy::bounded(NonZeroUsize::new(concurrency).unwrap());
            execution.queue_capacity = queue;
            execution.queue_timeout = Duration::from_secs(30);
            execution.startup_timeout = Duration::from_secs(10);
            execution.execution_timeout = Duration::from_secs(20);
            let policy = HostPolicy {
                id: "worker".parse().unwrap(),
                lifetime,
                execution,
            };
            self.hosts
                .configure(&self.manifests, &[policy.try_into().unwrap()], generation)
                .unwrap();
        }

        fn call(&self) -> Call {
            let hosts = self.hosts.clone();
            let signature = self.endpoint.signature();
            tokio::spawn(async move {
                hosts
                    .invoke(
                        "worker",
                        "test.run".parse().unwrap(),
                        Vec::new(),
                        &signature,
                        true,
                    )
                    .await
            })
        }

        async fn within<T>(future: impl Future<Output = T>) -> T {
            tokio::time::timeout(Duration::from_secs(10), future)
                .await
                .expect("lifecycle operation timed out")
        }

        async fn until(&self, description: &str, mut condition: impl FnMut() -> bool) {
            Self::within(async {
                while !condition() {
                    assert!(
                        !self.shutdown.is_triggered(),
                        "daemon stopped while waiting for {description}"
                    );
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await;
        }

        async fn phase(&self, phase: &str, count: usize) {
            self.until(phase, || {
                self.hosts
                    .executions("worker", "test.run")
                    .iter()
                    .filter(|e| e.phase == phase)
                    .count()
                    == count
            })
            .await;
        }

        fn processes(&self) -> Vec<omega_proto::omega::CommandProcessStatus> {
            self.hosts
                .inner
                .processes
                .inspect(&"worker".parse().unwrap())
        }

        fn connect(&self, id: u64) -> Peer {
            let (sender, requests) = mpsc::channel(64);
            Peer {
                _connection: self
                    .hosts
                    .connect(ProcessId::try_from(id).unwrap(), sender)
                    .unwrap(),
                requests,
            }
        }

        async fn success(call: Call) {
            assert_eq!(
                Self::within(call).await.unwrap().unwrap(),
                CommandAnswer::Acknowledged
            );
        }

        async fn refusal(call: Call, code: ErrorCode) {
            assert_eq!(Self::within(call).await.unwrap().unwrap_err().code, code);
        }

        async fn stop(&self) {
            self.shutdown.trigger();
            Self::within(async {
                while !self.hosts.all_stopped() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await;
            assert!(self.shutdown.failure().is_none());
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.shutdown.trigger();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn catalogue_failure_history_respects_the_callers_command_grants() {
        let registry = crate::plugins::PluginRegistry::detached(crate::hub::Hub::new());
        let mut fixture = Fixture::new();
        fixture.hosts = registry.hosts().clone();
        fixture.hosts.environment(
            Socket::at(fixture.root.join("control.sock")),
            fixture.shutdown.clone(),
        );
        let mut manifest = fixture
            .manifests
            .get(&"worker".parse().unwrap())
            .unwrap()
            .manifest
            .clone();
        let private = CommandEndpoint {
            id: "test.private".into(),
            ..fixture.endpoint.clone()
        };
        manifest.commands.push(private.clone());
        fixture.manifests = ManifestStore::from_manifests([manifest]);
        registry.adopt(&fixture.manifests);
        let generation = fixture.generation(None);
        fixture.configure(&generation, Lifetime::OneShot, 1, 0);
        Fixture::refusal(fixture.call(), ErrorCode::Unavailable).await;
        let refusal = Fixture::within(fixture.hosts.invoke(
            "worker",
            "test.private".parse().unwrap(),
            Vec::new(),
            &private.signature(),
            true,
        ))
        .await
        .unwrap_err();
        assert_eq!(refusal.code, ErrorCode::Unavailable);
        assert_eq!(
            registry.command_catalogue(None).unwrap().hosts[0]
                .recent_failures
                .len(),
            2
        );

        let mut caller = Manifest::new(&"caller".parse().unwrap(), "1");
        caller
            .command_dependencies
            .push(omega_proto::omega::CommandDependency {
                command: "test.run".into(),
                signature: fixture.endpoint.signature(),
            });
        let grants = crate::authorization::Grants::of(&caller).unwrap();
        let catalogue = registry.command_catalogue(Some(&grants)).unwrap();
        assert_eq!(catalogue.entries.len(), 1);
        assert_eq!(catalogue.hosts[0].recent_failures.len(), 1);
        assert_eq!(catalogue.hosts[0].recent_failures[0].command, "test.run");
        fixture.stop().await;
    }

    #[tokio::test]
    async fn eager_startup_failure_is_visible_without_any_invocation() {
        let fixture = Fixture::new();
        let generation = fixture.generation(None);
        tokio::time::pause();
        fixture.configure(&generation, Lifetime::Persistent(StartPolicy::Eager), 1, 0);
        fixture
            .until("startup failure", || {
                !fixture.hosts.inspect()[0].startup_error.is_empty()
            })
            .await;
        let status = &fixture.hosts.inspect()[0];
        assert_eq!(status.phase, CommandHostPhase::Backoff as i32);
        assert!(status.retry_after_ms.is_some());
        assert_eq!(status.start, omega_proto::omega::HostStart::Eager as i32);
        assert!(status.recent_failures.is_empty());
        assert_eq!((status.queued_calls, status.active_calls), (0, 0));
        tokio::time::resume();
        fixture.stop().await;
    }

    #[tokio::test]
    async fn backoff_refusals_preserve_the_original_startup_failure() {
        let fixture = Fixture::new();
        let generation = fixture.generation(None);
        fixture.configure(
            &generation,
            Lifetime::Persistent(StartPolicy::OnDemand),
            1,
            0,
        );
        tokio::time::pause();
        Fixture::refusal(fixture.call(), ErrorCode::Unavailable).await;
        let original = fixture.hosts.inspect()[0].startup_error.clone();
        assert!(!original.is_empty());
        Fixture::refusal(fixture.call(), ErrorCode::Unavailable).await;
        let status = &fixture.hosts.inspect()[0];
        assert_eq!(status.startup_error, original);
        assert_eq!(status.phase, CommandHostPhase::Backoff as i32);
        assert_eq!(status.recent_failures.len(), 2);
        tokio::time::resume();
        fixture.stop().await;
    }

    #[tokio::test]
    async fn simultaneous_first_calls_share_one_starting_process() {
        let fixture = Fixture::new();
        let generation = fixture.generation(Some(Fixture::PROGRAM));
        fixture.configure(
            &generation,
            Lifetime::Persistent(StartPolicy::OnDemand),
            4,
            0,
        );
        let calls: Vec<_> = (0..4).map(|_| fixture.call()).collect();
        fixture.phase("starting", 4).await;
        let processes = fixture.processes();
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].phase, "starting");
        let mut peer = fixture.connect(processes[0].id);
        for _ in 0..4 {
            Peer::complete(peer.request().await);
        }
        for call in calls {
            Fixture::success(call).await;
        }
        assert!(
            fixture
                .hosts
                .executions("worker", "test.run")
                .iter()
                .all(|e| e.process_id == processes[0].id && e.outcome == "ok")
        );
        fixture.stop().await;
    }

    #[tokio::test]
    async fn replacement_drains_old_calls_and_retains_their_generation() {
        let fixture = Fixture::new();
        let old_generation = fixture.generation(Some(Fixture::PROGRAM));
        let old_id = old_generation.id().clone();
        fixture.configure(
            &old_generation,
            Lifetime::Persistent(StartPolicy::OnDemand),
            1,
            1,
        );
        let first = fixture.call();
        fixture.phase("starting", 1).await;
        let old_process = fixture.processes()[0].id;
        let mut old_peer = fixture.connect(old_process);
        let first_request = old_peer.request().await;
        let queued = fixture.call();
        fixture.phase("queued", 1).await;

        let new_generation = fixture.generation(Some(Fixture::PROGRAM));
        fixture.configure(
            &new_generation,
            Lifetime::Persistent(StartPolicy::OnDemand),
            1,
            1,
        );
        drop(old_generation);
        let new_call = fixture.call();
        fixture.phase("starting", 1).await;
        let processes = fixture.processes();
        assert_eq!(processes.len(), 2);
        let new_process = processes.iter().find(|p| p.id != old_process).unwrap().id;
        let mut new_peer = fixture.connect(new_process);
        Peer::complete(new_peer.request().await);
        Fixture::success(new_call).await;
        let status = &fixture.hosts.inspect()[0];
        assert_eq!((status.queued_calls, status.active_calls), (1, 1));
        assert!(
            !Generations::new(&fixture.layout)
                .clean()
                .unwrap()
                .contains(&old_id)
        );

        Peer::complete(first_request);
        Fixture::success(first).await;
        Peer::complete(old_peer.request().await);
        Fixture::success(queued).await;
        fixture
            .until("old process reaped", || {
                !fixture.processes().iter().any(|p| p.id == old_process)
            })
            .await;
        let executions = fixture.hosts.executions("worker", "test.run");
        assert_eq!(
            executions.iter().map(|e| e.process_id).collect::<Vec<_>>(),
            [old_process, old_process, new_process]
        );
        fixture
            .until("old generation released", || {
                Generations::new(&fixture.layout)
                    .clean()
                    .unwrap()
                    .contains(&old_id)
            })
            .await;
        fixture.stop().await;
    }

    #[tokio::test]
    async fn abandoned_running_call_keeps_capacity_until_completion() {
        let fixture = Fixture::new();
        let generation = fixture.generation(Some(Fixture::PROGRAM));
        fixture.configure(
            &generation,
            Lifetime::Persistent(StartPolicy::OnDemand),
            1,
            0,
        );
        let abandoned = fixture.call();
        fixture.phase("starting", 1).await;
        let process = fixture.processes()[0].id;
        let mut peer = fixture.connect(process);
        let request = peer.request().await;
        abandoned.abort();
        assert!(abandoned.await.unwrap_err().is_cancelled());
        Fixture::refusal(fixture.call(), ErrorCode::ResourceExhausted).await;
        assert_eq!(
            fixture.hosts.executions("worker", "test.run")[0].phase,
            "dispatched"
        );
        Peer::complete(request);
        fixture.phase("finished", 1).await;
        let next = fixture.call();
        Peer::complete(peer.request().await);
        Fixture::success(next).await;
        assert_eq!(fixture.processes()[0].id, process);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn spawn_failure_and_exit_before_handshake_release_capacity() {
        for program in [None, Some(b"#!/bin/sh\nexit 1\n".as_slice())] {
            let fixture = Fixture::new();
            let broken = fixture.generation(program);
            fixture.configure(&broken, Lifetime::OneShot, 1, 0);
            Fixture::refusal(fixture.call(), ErrorCode::Unavailable).await;
            assert!(fixture.processes().is_empty());
            assert_eq!(fixture.hosts.inner.process_slots.available_permits(), 32);
            assert_eq!(
                fixture.hosts.inner.bytes.available_permits(),
                8 * 1024 * 1024
            );
            let fixed = fixture.generation(Some(Fixture::PROGRAM));
            fixture.configure(&fixed, Lifetime::Persistent(StartPolicy::OnDemand), 1, 0);
            let next = fixture.call();
            fixture.phase("starting", 1).await;
            let mut peer = fixture.connect(fixture.processes()[0].id);
            Peer::complete(peer.request().await);
            Fixture::success(next).await;
            fixture.stop().await;
        }
    }

    #[tokio::test]
    async fn startup_deadline_reaps_unconnected_children_and_allows_later_calls() {
        for lifetime in [
            Lifetime::OneShot,
            Lifetime::Persistent(StartPolicy::OnDemand),
        ] {
            let fixture = Fixture::new();
            let generation = fixture.generation(Some(Fixture::PROGRAM));
            fixture.configure(&generation, lifetime, 1, 0);
            let call = fixture.call();
            fixture.phase("starting", 1).await;
            let first_process = fixture.processes()[0].id;
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(11)).await;
            tokio::time::resume();
            Fixture::refusal(call, ErrorCode::Unavailable).await;
            assert!(fixture.processes().is_empty());
            let status = &fixture.hosts.inspect()[0];
            assert_eq!(status.phase, CommandHostPhase::Failed as i32);
            assert_eq!(status.startup_error, "handshake timed out");
            assert_eq!((status.queued_calls, status.active_calls), (0, 0));
            assert_eq!(status.recent_failures[0].command, "test.run");
            let next = fixture.call();
            fixture.phase("starting", 1).await;
            let new_process = fixture.processes()[0].id;
            assert_ne!(first_process, new_process);
            let mut peer = fixture.connect(new_process);
            let status = &fixture.hosts.inspect()[0];
            assert_eq!(status.phase, CommandHostPhase::Running as i32);
            assert!(status.startup_error.is_empty());
            Peer::complete(peer.request().await);
            Fixture::success(next).await;
            fixture.stop().await;
        }
    }

    #[tokio::test]
    async fn uncooperative_child_holds_capacity_through_forced_shutdown() {
        let fixture = Fixture::new();
        let generation = fixture.generation(Some(
            b"#!/bin/sh\ntrap '' TERM\nprintf 'ready\\n'\nexec /bin/sleep 300\n",
        ));
        fixture.configure(&generation, Lifetime::OneShot, 1, 0);
        let call = fixture.call();
        fixture.phase("starting", 1).await;
        let log = fixture.layout.plugin_log(&"worker".parse().unwrap());
        fixture
            .until("child ignores SIGTERM", || {
                std::fs::read_to_string(&log).is_ok_and(|log| log.contains("ready"))
            })
            .await;
        let mut peer = fixture.connect(fixture.processes()[0].id);
        let request = peer.request().await;

        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(21)).await;
        fixture
            .until("stopping child", || {
                fixture.processes()[0].phase == "stopping"
            })
            .await;
        assert!(!call.is_finished());
        Fixture::refusal(fixture.call(), ErrorCode::ResourceExhausted).await;
        assert!(request.answer.is_closed());
        tokio::time::advance(crate::process::ManagedChild::GRACE + Duration::from_millis(1)).await;
        tokio::time::resume();

        Fixture::refusal(call, ErrorCode::OutcomeUnknown).await;
        assert!(fixture.processes().is_empty());
        assert_eq!(fixture.hosts.inner.process_slots.available_permits(), 32);
        assert_eq!(
            fixture.hosts.inner.bytes.available_permits(),
            8 * 1024 * 1024
        );
        fixture.stop().await;
    }

    #[tokio::test]
    async fn execution_deadline_reaps_process_before_releasing_capacity_without_replay() {
        for lifetime in [
            Lifetime::OneShot,
            Lifetime::Persistent(StartPolicy::OnDemand),
        ] {
            let fixture = Fixture::new();
            let generation = fixture.generation(Some(Fixture::PROGRAM));
            fixture.configure(&generation, lifetime, 1, 0);
            let call = fixture.call();
            fixture.phase("starting", 1).await;
            let first_process = fixture.processes()[0].id;
            let mut peer = fixture.connect(first_process);
            let outstanding = peer.request().await;
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(21)).await;
            tokio::time::resume();
            Fixture::refusal(call, ErrorCode::OutcomeUnknown).await;
            assert!(fixture.processes().is_empty());
            assert!(outstanding.answer.is_closed());
            assert!(peer.requests.try_recv().is_err());
            let next = fixture.call();
            fixture.phase("starting", 1).await;
            let new_process = fixture.processes()[0].id;
            assert_ne!(first_process, new_process);
            let mut next_peer = fixture.connect(new_process);
            Peer::complete(next_peer.request().await);
            Fixture::success(next).await;
            fixture.stop().await;
        }
    }
}
