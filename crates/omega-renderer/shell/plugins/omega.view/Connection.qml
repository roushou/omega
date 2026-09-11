pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import "Props.js" as Props

// The daemon's shell socket, and the view this host is drawing.
//
// Everything below a host's layout is protocol: which line is a view, which
// view is *this* one, what to do when the daemon goes quiet, and how a press
// travels back. A bar widget and a panel need all of it and differ only in
// chrome, so it lives here once rather than being copied into the second host
// and drifting from the first.
Item {
    id: link

    // A view is addressed by three things: the unit that publishes it, the
    // surface within that unit, and — when a surface is instantiated more
    // than once by the state document — which instance.
    //
    // Unit and surface are filters, and empty means "whatever is publishing"
    // — so a plugin somebody has just scaffolded draws without being
    // configured first, and naming one narrows it.
    property string unit: ""
    property string surface: ""
    property string module: ""

    // Where the daemon listens, unless a host was configured with somewhere
    // else. Named separately so a host can fall back to it explicitly: a host
    // that repeated the path would be a second place for it to be wrong.
    readonly property string defaultSocketPath:
        Quickshell.env("XDG_RUNTIME_DIR") + "/omega-shell.sock"
    property string socketPath: link.defaultSocketPath

    // The tree currently published for this surface. Null until the first one
    // arrives, and null again for a unit that decided to draw nothing.
    property var tree: null
    // Which unit published the tree currently drawn. A press goes back to it.
    property string drawnBy: ""
    property bool connected: false
    property var units: []
    readonly property string status: {
        if (!link.connected) return "Waiting for Omega…"
        var name = link.unit || link.drawnBy
        if (!name) return ""
        for (var i = 0; i < link.units.length; i++) {
            var unit = link.units[i]
            if (unit.unit !== name) continue
            switch (unit.phase) {
                case "UNIT_PHASE_RUNNING": return ""
                case "UNIT_PHASE_STARTING": return "Starting plugin…"
                case "UNIT_PHASE_RESTARTING": return "Restarting plugin…"
                case "UNIT_PHASE_STOPPED": return "Plugin stopped."
                case "UNIT_PHASE_FAILED": return unit.detail || "Plugin failed to start."
                default: return "Plugin status unavailable."
            }
        }
        return "Waiting for plugin…"
    }

    function disconnected(reason) {
        link.connected = false
        link.tree = null
        link.drawnBy = ""
        link.units = []
        requests.disconnected(reason)
    }
    // When the daemon last said anything. A socket whose peer went away does
    // not reliably report itself closed, so silence is what we watch instead.
    property double lastHeard: 0
    property int nextStream: 1
    readonly property alias requests: requests
    Requests { id: requests }

    function onLine(line) {
        link.lastHeard = Date.now()

        var msg
        try {
            msg = JSON.parse(String(line))
        } catch (e) {
            return
        }

        // An answer to something this host asked. A press that was refused
        // said why, and a button that silently does nothing is the worst way
        // to find that out.
        if (msg.result) {
            requests.finish(msg.streamId, msg.result)
            return
        }

        if (msg.units) {
            link.units = msg.units.units || []
            return
        }

        // Only a view line describes a surface. Topics share this stream, and
        // a filter that is empty matches them too — which cleared the tree on
        // every state change the daemon published.
        if (!msg.view) return

        if (link.unit !== "" && msg.unit !== link.unit) return
        if (link.surface !== "" && msg.surface !== link.surface) return
        if ((msg.module || "") !== link.module) return
        link.drawnBy = msg.unit
        link.tree = msg.view && msg.view.root ? msg.view.root : null
    }

    function allocateStream() {
        var stream = link.nextStream
        link.nextStream += 2
        return stream
    }

    function subscribeToUnits() {
        link.send({
            streamId: link.allocateStream(),
            invoke: { subscribe: { topics: ["units"], events: [], replace: true } }
        })
    }

    // One write path, guarded: the socket is rebuilt on every retry, so there
    // are moments when there is no object to write to.
    function send(message) {
        var open = socketLoader.item as Socket
        if (!open || !open.connected) return false
        open.write(JSON.stringify(message) + "\n")
        return true
    }

    // A press is the operator asking this unit to run one of its own
    // commands: the same request the CLI makes, and the same one the daemon
    // authorizes by uid. It goes as JSON because QML cannot encode protobuf —
    // one protocol, two encodings.
    //
    // `bound.args` arrives as protobuf JSON `Value`s and an `InvokeUnit`
    // carries protobuf JSON `Value`s, so they are forwarded verbatim rather
    // than re-encoded here. A binding with no args sends none.
    //
    // A control that carries a value of its own — where a slider landed,
    // which way a toggle went — appends it *after* those, so a unit reads the
    // arguments it chose by position and the user's value last.
    function press(bound, value, key) {
        if (!bound || !bound.command) return false
        if (!link.connected || link.drawnBy === "") {
            requests.error = "Not connected to the plugin; command was not sent."
            return false
        }

        var args = (bound.args || []).slice()
        if (value !== undefined) {
            var encoded = Props.encode(value)
            if (encoded === null) { requests.error = "Unsupported control value."; return false }
            args.push(encoded)
        }
        var stream = link.allocateStream()
        if (!requests.begin(stream, key, Date.now())) return false
        var sent = link.send({
            streamId: stream,
            invoke: {
                act: {
                    action: {
                        // The unit that drew what was pressed, which is not
                        // necessarily the one this host was configured with.
                        invokeUnit: {
                            unit: link.drawnBy,
                            command: bound.command,
                            args: args
                        }
                    }
                }
            }
        })
        if (!sent) {
            requests.finish(stream, { done: true, error: { message: "Not connected; command was not sent." } })
            return false
        }
        return true
    }

    Component {
        id: socketComponent

        Socket {
            path: link.socketPath
            parser: SplitParser {
                onRead: function(line) { link.onLine(line) }
            }
            // `connected: true` as a static binding can fire before `path` is
            // set; connect after the component is fully initialized instead.
            Component.onCompleted: {
                link.lastHeard = Date.now()
                connected = true
            }

            // A daemon that went away is not a daemon still saying 91%.
            // Holding the last tree would leave the bar showing a reading
            // nobody is taking.
            onConnectedChanged: {
                if (connected) {
                    link.connected = true
                    link.subscribeToUnits()
                } else {
                    link.disconnected()
                }
            }
        }
    }

    Loader {
        id: socketLoader
        sourceComponent: socketComponent
    }

    // Recover from a daemon this host cannot currently reach.
    //
    // Not by asking whether the socket is connected: a peer that goes away
    // leaves `connected` reading true, and a host that trusts it freezes on
    // the last view it ever saw — which is worse than showing nothing,
    // because it looks like a working widget reporting a stale number.
    //
    // Silence is the signal instead. The daemon sends everything it holds the
    // moment a connection opens, so reconnecting after a quiet stretch costs
    // one snapshot.
    //
    // The retry rebuilds the `Socket` rather than toggling `connected` on the
    // existing one. A socket whose first connect found no file stays down
    // through every later toggle, so a host loaded before the daemon — which
    // is the order `omega init` installs them in — would never draw at all.
    // Reactivation is deferred a turn so the destroy and the create cannot
    // coalesce into no change.
    Timer {
        interval: 5000
        running: true
        repeat: true
        onTriggered: {
            var expired = requests.expire(Date.now())
            if (Date.now() - link.lastHeard < 15000 && !expired) return
            link.disconnected(expired ? requests.error : "")
            link.lastHeard = Date.now()
            socketLoader.active = false
            Qt.callLater(function() { socketLoader.active = true })
        }
    }
}
