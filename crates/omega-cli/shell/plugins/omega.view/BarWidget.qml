import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui

// Renders the declarative view tree an Omega plugin publishes for a surface.
// Connects straight to the daemon's shell socket (JSON lines) — no bridge.
//
// The tree is drawn by `ViewNode`, which instantiates one item per node. A
// press travels back up to here and out as a request on the same socket: the
// operator asking the plugin to run one of its own commands, which is the
// same request `omega run` makes.
BarWidget {
  id: root
  moduleName: "omega.view"

  // A view is addressed by three things: the unit that publishes it, the
  // surface within that unit, and — when a surface is instantiated more than
  // once by the state document — which instance.
  //
  // Unit and surface are filters, and empty means "whatever is publishing" —
  // so a plugin somebody has just scaffolded draws without being configured
  // first, and naming one narrows it.
  //
  // `module` names one instance when a document declared several. Empty
  // draws whichever instance is publishing, which is safe because the daemon
  // forgets a unit's views when it goes: every view on the stream belongs to
  // a plugin that is running now.
  readonly property string unit: setting("unit", "")
  readonly property string surface: setting("surface", "")
  readonly property string module: setting("module", "")
  readonly property string socketPath: setting("socket", Quickshell.env("XDG_RUNTIME_DIR") + "/omega-shell.sock")

  // The tree currently published for this surface. Null until the first one
  // arrives, and null again for a plugin that decided to draw nothing.
  property var tree: null
  // When the daemon last said anything. A socket whose peer went away does
  // not reliably report itself closed, so silence is what we watch instead.
  property double lastHeard: 0
  // Which unit published the tree currently drawn. A press goes back to it.
  property string drawnBy: ""
  property int nextStream: 1

  function onLine(line) {
    root.lastHeard = Date.now()

    var msg
    try {
      msg = JSON.parse(String(line))
    } catch (e) {
      return
    }

    // An answer to something this widget asked. A press that was refused said
    // why, and a button that silently does nothing is the worst way to find
    // that out.
    if (msg.result) {
      if (msg.result.error)
        console.warn("omega:", root.drawnBy, "refused:", msg.result.error.message)
      return
    }

    // Only a view line describes a surface. Topics share this stream, and a
    // filter that is empty matches them too — which cleared the tree on
    // every state change the daemon published.
    if (!msg.view) return

    if (root.unit !== "" && msg.unit !== root.unit) return
    if (root.surface !== "" && msg.surface !== root.surface) return
    if (root.module !== "" && (msg.module || "") !== root.module) return
    root.drawnBy = msg.unit
    root.tree = msg.view && msg.view.root ? msg.view.root : null
  }

  // A press is the operator asking this unit to run one of its own commands:
  // the same request the CLI makes, and the same one the daemon authorizes by
  // uid. It goes as JSON because QML cannot encode protobuf — one protocol,
  // two encodings.
  function press(command) {
    if (!command || root.drawnBy === "") return
    shell.write(JSON.stringify({
      streamId: root.nextStream++,
      invoke: {
        act: {
          action: {
            // The unit that drew what was pressed, which is not necessarily
            // the one this widget was configured with.
            invokeUnit: { unit: root.drawnBy, command: command, args: [] }
          }
        }
      }
    }) + "\n")
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Socket {
    id: shell
    path: root.socketPath
    parser: SplitParser {
      onRead: function(line) { root.onLine(line) }
    }
    // `connected: true` as a static binding can fire before `path` is set;
    // connect after the component is fully initialized instead.
    Component.onCompleted: {
      root.lastHeard = Date.now()
      connected = true
    }

    // A daemon that went away is not a daemon still saying 91%. Holding the
    // last tree would leave the bar showing a reading nobody is taking.
    onConnectedChanged: if (!connected) {
      root.tree = null
      root.drawnBy = ""
    }
  }

  // Recover from a daemon that restarted.
  //
  // Not by asking whether the socket is connected: a peer that goes away
  // leaves `connected` reading true, and a widget that trusts it freezes on
  // the last view it ever saw — which is worse than showing nothing, because
  // it looks like a working widget reporting a stale number.
  //
  // Silence is the signal instead. The daemon sends everything it holds the
  // moment a connection opens, so reconnecting when nothing has been said
  // for a while costs one snapshot and fixes every way this can break.
  Timer {
    interval: 5000
    running: true
    repeat: true
    onTriggered: {
      if (Date.now() - root.lastHeard < 15000) return
      root.lastHeard = Date.now()
      shell.connected = false
      shell.connected = true
    }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    // The tree draws itself; the button is the bar's chrome around it —
    // hover, tooltip, and the slot the bar lays out.
    labelVisible: false
    hasVisualContent: root.tree !== null
    // A button with no label is a button with no width, and the bar lays out
    // by implicit size — so the slot has to follow the tree instead.
    fixedWidth: view.implicitWidth > 0
      ? view.implicitWidth + button.scaledHorizontalMargin * 2
      : 0

    ViewNode {
      id: view
      anchors.centerIn: parent
      model: root.tree
      foreground: button.foreground
      visible: root.tree !== null
      onInvoke: (command) => root.press(command)
    }
  }
}
