pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import "Props.js" as Props

Item {
    id: link

    property string unit: ""
    property string surface: ""
    property string module: ""
    property bool standalone: false
    property bool attached: false
    property var instance: null
    property var revision: "0"
    property bool presented: false
    property var instances: ({})
    property int attachmentStream: 0
    signal viewUpdated(var snapshot)

    // Where the daemon listens, unless a host was configured with somewhere
    // else. Named separately so a host can fall back to it explicitly: a host
    // that repeated the path would be a second place for it to be wrong.
    readonly property string defaultSocketPath:
        Quickshell.env("XDG_RUNTIME_DIR") + "/omega-shell.sock"
    property string socketPath: link.defaultSocketPath

    // The tree currently published for this surface. Null until the first one
    // arrives, and null again for a unit that decided to draw nothing.
    property var tree: null
    property var snapshot: null
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
        link.snapshot = null
        link.attached = false
        link.instance = null
        link.instances = ({})
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
            if (Number(msg.streamId) === link.attachmentStream) {
                if (msg.result.error) {
                    requests.error = msg.result.error.message
                    return
                }
                link.attached = true
                var snapshots = msg.result.instances ? msg.result.instances.instances || [] : []
                for (var n = 0; n < snapshots.length; n++) link.receiveView(snapshots[n])
            }
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
        if (msg.view && link.attached) link.receiveView(msg)
    }

    function receiveView(msg) {
        if (!msg.instance) return
        if (link.unit !== "" && msg.unit !== link.unit) return
        if (!link.standalone && msg.surface !== link.surface) return
        var copy = Object.assign({}, link.instances)
        if (msg.destroyed) delete copy[msg.instance.id]
        else copy[msg.instance.id] = msg
        link.instances = copy
        link.snapshot = msg
        link.instance = msg.instance
        link.revision = msg.view ? msg.view.revision || "0" : "0"
        link.presented = msg.requested === 2 || msg.requested === "PRESENTATION_STATE_VISIBLE"
        link.drawnBy = msg.unit
        link.tree = msg.view && msg.view.root ? msg.view.root : null
        link.viewUpdated(msg)
    }

    function attach() {
        if (!link.unit || (!link.standalone && (!link.surface || !link.module))) return
        link.attachmentStream = link.allocateStream()
        var request = { features: ["RENDERER_FEATURE_INSTANCES", "RENDERER_FEATURE_SCOPED_INTERACTIONS", "RENDERER_FEATURE_LOCAL_MESSAGES", "RENDERER_FEATURE_CONTROLLED_INPUTS"] }
        if (link.standalone) {
            request.unit = link.unit
            request.features.push("RENDERER_FEATURE_WINDOWS", "RENDERER_FEATURE_OVERLAYS")
        } else {
            request.placement = { unit: link.unit, surface: link.surface, placement: link.module }
            request.features.push("RENDERER_FEATURE_EMBEDDED", "RENDERER_FEATURE_POPUPS")
        }
        link.send({ streamId: link.attachmentStream, invoke: { attachRenderer: request } })
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
        var open = link.socket
        if (!open || !open.connected) return false
        open.write(JSON.stringify(message) + "\n")
        return true
    }

    function busy(key) { return link.instance ? requests.busy(link.instance.id + "/" + key) : false }

    function press(bound, value, key, event) {
        return link.interact(link.instance, link.revision, key, event, value)
    }

    function interact(identity, revision, key, event, value) {
        if (!link.attached || !identity) {
            requests.error = "Not attached; interaction was not sent."
            return false
        }
        var interaction = { instance: identity, revision: String(revision), node: key, event: event }
        if (value !== undefined) {
            var encoded = Props.encode(value)
            if (encoded === null) { requests.error = "Unsupported control value."; return false }
            interaction.value = encoded
        }
        var stream = link.allocateStream()
        if (!requests.begin(stream, identity.id + "/" + key, Date.now())) return false
        if (!link.send({ streamId: stream, invoke: { interact: interaction } })) {
            requests.finish(stream, { done: true, error: { message: "Disconnected; interaction was not sent." } })
            return false
        }
        return true
    }

    function change(action) {
        if (!link.attached || !link.instance) return
        link.send({ streamId: link.allocateStream(), invoke: { changePresentation: { instance: link.instance, action: action } } })
    }

    function report(identity, state) {
        if (!link.attached || !identity) return
        link.send({ streamId: link.allocateStream(), invoke: { reportPresentation: { instance: identity, observed: state } } })
    }

    Component {
        id: socketComponent

        Socket {
            path: link.socketPath
            parser: SplitParser {
                onRead: function(line) { link.onLine(line) }
            }
            // A daemon that went away is not a daemon still saying 91%.
            // Holding the last tree would leave the bar showing a reading
            // nobody is taking.
            onConnectedChanged: {
                if (connected) {
                    link.connected = true
                    link.subscribeToUnits()
                    link.attach()
                } else {
                    link.disconnected()
                }
            }
        }
    }

    property Socket socket: null

    function openSocket() {
        // Qt 6.4 Loader creates a context incompatible with this bound component.
        link.socket = socketComponent.createObject(link)
        // Both path and the owning reference must exist before connection signals fire.
        link.lastHeard = Date.now()
        link.socket.connected = true
    }

    function reconnect(reason) {
        link.disconnected(reason)
        link.lastHeard = Date.now()
        if (link.socket) link.socket.destroy()
        link.socket = null
        Qt.callLater(link.openSocket)
    }

    Component.onCompleted: link.openSocket()

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
            if (link.connected && !link.attached) { link.attach(); return }
            if (Date.now() - link.lastHeard < 15000 && !expired) return
            link.reconnect(expired ? requests.error : "")
        }
    }
}
