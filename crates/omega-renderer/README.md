# Omega renderer

Host-independent QML controls for Omega view trees. `Core::FILES` embeds the shared
nodes, theme, asset resolver, request tracking, and generated prop/icon readers.
This crate depends only on the protocol. Omarchy authoring, transport, and renderer
installation live in [omega-omarchy](../omega-omarchy).

A `ViewNode` accepts a tree, theme, asset resolver, interaction session, and its
allocated width/height. Nested nodes preserve these inputs through bindings.
A default theme is supplied; the Omarchy adapter binds the same inputs to its
existing theme. Image sources never fetch network resources implicitly.

An isolated fixture window uses these same nodes:

```sh
quickshell -p crates/omega-renderer/shell
```

Choose normal, loading, long-label, or disabled states; resize the window and use
the keyboard. Interactions are captured locally and never reach the daemon. This
is the renderer development harness. For Rust component and surface cases, use
`omega preview <package>`; see the [preview guide](../../docs/previews.md).
The crate also embeds the standalone production host and isolated preview host.

```sh
crates/omega-renderer/shell/lint.sh
crates/omega-renderer/shell/test.sh
```

Licensed under MIT.
