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
    // When the daemon last said anything. A socket whose peer went away does
    // not reliably report itself closed, so silence is what we watch instead.
    property double lastHeard: 0
    property int nextStream: 1

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
            if (msg.result.error)
                console.warn("omega:", link.drawnBy, "refused:", msg.result.error.message)
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

    // This host draws views and reads no state, so it asks for none.
    //
    // `replace` rather than a list of what to drop: the daemon holds every
    // topic there is, and naming the unwanted ones would let each new one
    // back in as the ontology grows. An empty selection stays empty.
    //
    // The daemon sends everything it holds the moment a connection opens, so
    // this narrows what follows rather than what arrived — one snapshot, and
    // then only the views this host is here for.
    function allocateStream() {
        var stream = link.nextStream
        link.nextStream += 2
        return stream
    }

    function subscribeToNothing() {
        link.send({
            streamId: link.allocateStream(),
            invoke: { subscribe: { topics: [], events: [], replace: true } }
        })
    }

    // One write path, guarded: the socket is rebuilt on every retry, so there
    // are moments when there is no object to write to.
    function send(message) {
        var open = socketLoader.item as Socket
        if (!open) return
        open.write(JSON.stringify(message) + "\n")
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
    function press(bound, value) {
        if (!bound || !bound.command || link.drawnBy === "") return

        var args = (bound.args || []).slice()
        if (value !== undefined) {
            var encoded = Props.encode(value)
            if (encoded !== null) args.push(encoded)
        }
        link.send({
            streamId: link.allocateStream(),
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
                    link.subscribeToNothing()
                } else {
                    link.tree = null
                    link.drawnBy = ""
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
            if (Date.now() - link.lastHeard < 15000) return
            link.lastHeard = Date.now()
            socketLoader.active = false
            Qt.callLater(function() { socketLoader.active = true })
        }
    }
}
