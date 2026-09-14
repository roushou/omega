# Omega previews

Development-only component and surface cases, using Omega's production renderer
and isolated task runtime. Add `omega-preview` as a **dev dependency** in a plugin
or library, then register a normal Rust library test:

```rust
#[cfg(test)]
mod previews {
    #[test]
    fn preview() {
        omega_preview::Cases::new()
            .component("ready", || omega::ui::Text::new("Ready"))
            .component("disabled", || omega::ui::Button::new("Retry").disabled())
            .run().unwrap();
    }
}
```

`cargo test` validates initial renders. `omega preview <package>` opens the named
cases with viewport/theme controls, reset, and an event/effect inspector. Private
components are accessible because registration is in their own library test.
Production manifests and binaries contain no registrations.

Use `Cases::surface` with synthetic `omega::testing::State` for interactive
surfaces, or `surface_with` with an existing `SurfaceHarness` factory for settings
and initial messages. `Cases::draw` uses the same factory for structural assertions.
Effects wait for explicit simulated success/refusal; leaving one pending exercises
loading. There is no fallback to a daemon or live Omega backend.

This isolates Omega effects, not arbitrary Rust code. Fixtures and custom injected
services must be synthetic. Preview targets are ordinary trusted development code.
See [the preview guide](../../docs/previews.md) for capture and comparison.
