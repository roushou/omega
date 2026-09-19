# Component and surface previews

`omega preview` runs development cases from a plugin or library and renders them
with Omega's shared QML controls. It does not start or adopt a production plugin,
connect to the daemon, or install anything in the bar.

## Register cases

Add a workspace dependency and opt into it only from members that have previews:

```toml
# ~/.config/omega/Cargo.toml
[workspace.dependencies]
omega-preview = "0.4.0"

# A plugin or library's Cargo.toml
[dev-dependencies]
omega-preview.workspace = true
```

When using a checkout, run `omega link /path/to/omega` after adding the workspace
dependency. Linking includes the optional preview crate only when declared.

Registration is a normal Rust library test. It can access private components;
there is no production registration, filesystem routing, or component registry.

```rust
#[cfg(test)]
mod previews {
    #[test]
    fn preview() {
        omega_preview::Cases::new()
            .component("ready", || omega::ui::Text::new("Ready"))
            .component("long-label", || omega::ui::Text::new("A deliberately long label"))
            .component("disabled", || omega::ui::Button::new("Retry").disabled())
            .run().unwrap();
    }
}
```

`Cases::surface::<MySurface>("loading", State::new())` uses the actual
surface runtime with synthetic readings. `surface_with` accepts a factory returning
`SurfaceHarness<MySurface>` when cases need construction settings, initial messages,
or injected behavior dependencies. Reuse that factory in behavioral tests;
`Cases::draw("ready")` supports structural assertions against the same cases.
Normal `cargo test` validates every initial render and opens no window.

## Inspect and edit

```sh
omega preview launcher --case applications
omega preview desktop-ui --case level-control
omega preview desktop-ui --list
omega preview my-library --manifest-path /path/to/workspace/Cargo.toml
```

The default test is `previews::preview`; `--test another::test_name` selects a
different explicit registration. Cases have unique identifier names, with at most
128 per registration. `--width`, `--height`, and `--theme dark|light` set initial
viewport tokens. The window also provides controls for these, case selection, and
reset. `--no-watch` disables rebuilding.

A selected case starts with fresh model/task state. Reset or case replacement drops
its pending work. Changes in the workspace or resolved local path dependencies
trigger incremental Cargo builds. A failed build leaves the last successful tree
visible and marked stale, with interactions disabled; compiler diagnostics appear
in the terminal. Successful replacement gets fresh bindings and state. External
path dependencies added after starting a session require restarting the preview.

Effects never reach live Omega services. The inspector shows their kind and offers
explicit success or refusal. Leave a request pending to inspect loading; resolve
it later to inspect completion. Successful presentation effects apply the real surface lifecycle and hide its
viewport; the preview window stays open so you can inspect the result and reset. The event history retains kinds rather than
input values or operation arguments, and is capped at 64 entries.

This is not an OS sandbox. Fixture factories and injected services are ordinary
Rust code: keep them synthetic, including clock/randomness/filesystem dependencies.
A preview must not wrap a real service and call that a fixture. Rendering can show
whatever data the fixture supplied; deliberately avoid secrets in captured cases.

## Capture and compare

```sh
omega preview launcher --case applications --capture /tmp/launcher.png
omega preview launcher --case applications --capture /tmp/launcher.png \
  --baseline ./previews/launcher.png
# After reviewing the image/difference, explicitly approve a new baseline:
omega preview launcher --case applications --capture /tmp/launcher.png \
  --baseline ./previews/launcher.png --update-baseline
```

Capture requires an explicit case and runs offscreen. It captures the component
viewport, excluding the inspector. The environment fixes scale 1, 96 DPI, DejaVu
Sans, software rendering, viewport dimensions, and theme. Capture waits for image
loads and a settling interval, disables built-in motion/cursor blinking, and fails
on asset errors or failure to settle within 30 seconds. Fixture-owned periodic
changes still require a deterministic fixture; a settle interval is not a virtual
clock for arbitrary user code.

The PNG has a JSON sidecar recording the case, dimensions, theme, Quickshell
version, font hash, and loaded Qt/font raster library hashes. Comparisons refuse
mismatched environments. Matching environments compare decoded RGBA pixels exactly;
a mismatch returns failure and writes `<capture>.diff.png`, highlighting changed
pixels. Baselines are never created or replaced implicitly. Capture and baseline
must be different files.

Before writing a capture, Omega queries `quickshell --version` and `fc-match` for
the environment metadata. Each query has a 10-second timeout and a 64 KiB limit
per output stream. Failed or invalid responses stop the capture before it writes
the output PNG, metadata, or baseline.

Interactive previews use the production theme icon provider. Offscreen capture
requires fixed local fixture images instead: the native icon provider can produce
placeholder textures with a software backend. Capture refuses theme icons explicitly.
Local fixture assets also avoid desktop-theme differences between machines.
Offscreen captures do not test compositor placement, native input routing, portal
registration, or desktop opacity rules. Keep native host smoke tests separate.
