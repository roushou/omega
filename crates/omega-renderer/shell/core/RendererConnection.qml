pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import "Props.js" as Props

Item {
    id: link

    // Replaced with the complete bundle fingerprint during installation.
    readonly property string buildFingerprint: ""
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

    // Default daemon socket path, overridable by the host.
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
    // Track inbound activity to detect stale connections even without a socket close signal.
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

        // Route correlated results to pending controls.
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

        // Only view frames may replace the rendered tree.
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
        request.buildFingerprint = link.buildFingerprint
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

    function change(action, key) {
        if (!link.attached || !link.instance) {
            requests.error = "Not attached; presentation was not sent."
            return false
        }
        var stream = link.allocateStream()
        if (!requests.begin(stream, key, Date.now())) return false
        if (!link.send({ streamId: stream, invoke: { changePresentation: { instance: link.instance, action: action } } })) {
            requests.finish(stream, { done: true, error: { message: "Disconnected; presentation was not sent." } })
            return false
        }
        return true
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
            // Clear stale views when the daemon disconnects.
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

    // Reconnect after the heartbeat silence deadline.
    // Replace the Socket object after failure; toggling a failed socket can leave it
    // disconnected. Defer reactivation so destruction and creation cannot coalesce.
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
