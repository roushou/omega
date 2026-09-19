# Omega CLI

The `omega` command builds and runs a desktop configuration written in Rust.
It creates plugin projects, manages the daemon and renderer, and lets you inspect
plugins or run them against the live desktop during development.

Omega requires Linux with a systemd user manager and Rust 1.88 or newer.
Quickshell renders independent windows and overlays; Omarchy hosts bar placements. Install this package with
`cargo install omega-cli --locked`; see the
[project README](https://github.com/roushou/omega) for setup.

## Using Omega

```sh
omega init
omega new audio
```

`omega init` creates and builds a Rust workspace in `~/.config/omega`, installs
the renderer and daemon service, then verifies configuration and renderer activation. `omega new` scaffolds a plugin and prints
its placement declaration for `system/src/main.rs`. The
[plugin SDK](https://crates.io/crates/omega-rs) provides the types your plugin uses.

```sh
omega build
omega status
omega status network
omega status --versions
omega status --json
omega logs audio
```

A build compiles the configuration and plugins, then stages them for the daemon
to apply. Use `omega build --debug` for a debug build. `omega dev audio`
temporarily replaces the supervised plugin with a development process in your
terminal.

Inspect available endpoints with `omega commands` or `omega commands audio --json`.
The JSON output includes declared input/output shapes, signatures, descriptions,
and connection availability. Discovery does not guarantee a later call will succeed.

Command arguments are decoded by the plugin's input type. For plugins exposing
these commands:

```sh
omega run brightness set-level 40%
omega run power profile balanced
```

Percentages also accept fractions (`0.4`); boolean inputs accept `true` or `false`.
Numeric inputs accept numbers, including negatives. Text inputs stay literal,
so `001` or `true` remains text when the command expects a string.

Configuration errors identify duplicate placement locations and list available
widget surfaces. When `omega shell adopt` encounters malformed JSON, it shows
the offending source location. Diagnostics go to stderr, keeping command results
on stdout available for scripts.

`omega daemon` runs the daemon in the foreground. `omega daemon install` makes it
a user service, and `omega daemon status` checks the installed service. Service
installation and removal stop with a nonzero exit status when systemd rejects an
operation. Errors include the failed operation, systemd's output, and a diagnostic
command. Files already installed and any recovery records remain in place. A
failed stop/disable leaves the service file in place. Status distinguishes missing
files, stale definitions, failed services, and temporary enablement. A failed
manager query is reported as an error. A command timeout does not prove that the
systemd job stopped; inspect service status before retrying.

Use `omega link /path/to/omega` to build your configuration against a source checkout,
or `omega link --published` to return to registry dependencies.

`omega new <name>` starts with a minimal text widget. Use
`omega new power --template battery` for a battery widget with configurable
low-charge styling. Both templates include tests and print a placement declaration. New plugins are
registered as workspace members. Put the printed expression into your shell's
`Bar::left`, `Bar::center`, or `Bar::right`; running a plugin alone does not
place its widget on screen.

`omega new audio-commands --command-host` creates a reusable command library and
executable under `commands/audio-commands/`. It adds the Cargo member, executable
metadata, and system dependency, then prints the `.command_host(...)` call to add
to your document. The starter command echoes text:

```sh
omega build
omega run audio-commands.echo hello
```

The default host starts on its first invocation and stays running. Its commands
can be imported by plugins without importing another UI plugin.

`omega check` compiles the workspace, evaluates the Rust configuration, and
validates its placements and schedules without publishing a generation.
Configurations containing only native Omarchy widgets can be checked and built
without creating any Omega plugins.

`omega build` publishes a generation for asynchronous daemon activation.
Use `omega status` to see whether the latest build is active,
inspect activation failures and the last reconciliation pass, and see the last
shell application result alongside plugin health. A completed pass does not mean
every plugin is running. Shell application is reported independently: a conflict
does not prevent valid plugins from starting. Use `omega shell diff` to check
the current file for external changes.

Use `omega build --wait --timeout 30s` to wait for that build to be accepted and
its configuration applied. The timeout starts after publication; failure leaves
the build published and does not cancel activation. This checks the last
reconciliation pass and shell application, not ongoing plugin health.
`omega status` distinguishes background plugins, unplaced surfaces, surfaces
waiting for required readings, and completed renders. Process failures take
precedence and include the last failure and log path. **Ready** means a render is
available, including a deliberately empty view; it does not mean the shell is
connected or the presentation is visible.

Use `omega status <plugin>` for its declared surfaces, instances, missing readings,
requested and observed presentation state, and log path. A configured placement
whose instance has not started is waiting, not unplaced. An older plugin's empty
view has unknown readiness.

Human-readable status goes to stderr. `omega status --json` writes the daemon
snapshot, including generation IDs and plugin health, to stdout for scripts.
`omega status <plugin> --json` selects that plugin's process, health, and renderer
facts while retaining the deployment summary.

Use `omega status --versions` to inspect the CLI executable, the running daemon's
version, renderer installation, and resolved Omega dependency versions and sources.
Dependency inspection is offline and leaves the lockfile unchanged. It describes
the current config workspace; running plugins retain their last published build.
These details go to stderr alongside the usual plugin summary.

`omega shell diff` shows changed JSON paths and their current and built values,
then suggests adoption, application, or explicit overwrite as appropriate.
Array paths use positions so widget ordering remains visible.

`omega preview <package>` opens isolated development cases from a plugin or library,
including private components. It watches edits, keeps failed builds visibly stale,
and captures effects for explicit simulated outcomes. `--capture` and `--baseline`
compare deterministic viewport images. See [the preview guide](../../docs/previews.md)
for registration, reset, and fixture requirements.

## Initialization and recovery

`omega init` checks Rust, the systemd user manager, Omarchy, existing files, and
unfinished recovery records before setup. It imports an existing `shell.json`
into Rust and prints the original backup and ownership-receipt paths. Compilation
and document validation finish before installing desktop files. The new generation
is published after the daemon responds with the expected version.

Use `omega init --bare` for workspace files only, or `omega init --debug` to build
without release optimizations. Full initialization requires the supported desktop;
missing host services are errors with `--bare` guidance.

Each changed workspace file, service file, and renderer directory gets a private
recovery record. Repeating initialization preserves existing Rust sources and
skips unchanged file replacements. A failed step stops setup, leaves completed
work in place, and reports recovery paths.

The setup summary lists created, updated, and retained files, with recovery-record
paths and an undo command for each change. It also reports the original shell
backup and completed service, publication, and shell actions. Summaries describe
this invocation; `omega recovery list` includes older records.

Failures identify the step and cause through a `miette` diagnostic. Advice depends
on the failed operation: a Cargo error suggests fixing the sources, a service
failure points to systemd status and logs, and an interrupted file write names
the exact record to inspect. Failures after publication explain that the new
generation remains selected. Existing JSON source labels are preserved.

```sh
omega recovery list
omega recovery inspect <id>
omega recovery accept <id>   # confirm an interrupted write that finished
```

To undo a filesystem change, stop the daemon, then use `omega recovery restore <id>`.
It refuses files that differ from both recorded states. Restore related source
edits in reverse installation order and rebuild before restarting the daemon.
Service files require `systemctl --user daemon-reload`; renderer changes require
`omarchy restart shell`. Recovery does not undo service enablement or select a
previous generation; use `omega rollback` for generation selection.

To restore the original shell layout, stop the daemon, copy the printed
`shell-before-omega.json` backup over `shell.json`, and restart Omarchy. Keep the
daemon stopped until the Rust layout reflects the configuration you want.

A new workspace usually has no Omega placements. Initialization verifies installed
renderer files and says that live QML is unverified until a placement attaches.

Run `omega --help` or `omega <command> --help` for the full command reference.

[Project](https://github.com/roushou/omega) ·
[Architecture](https://github.com/roushou/omega/blob/main/docs/architecture.md)

Licensed under MIT.

## Storage (unreleased)

Inspect shared storage through the daemon. Answers are JSON on stdout:

```sh
omega storage list
omega storage show productivity.tasks --limit 50
omega storage export productivity.tasks > tasks.json
```

Use `--offline` to inspect persistent files while the daemon is stopped. The
command takes an exclusive lease and refuses to race an active storage owner.
See [shared storage](../../docs/storage.md) for declarations, access, limits,
and recovery.
