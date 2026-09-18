pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import "Props.js" as Props

QtObject {
    id: link

    // Replaced with the complete bundle fingerprint during installation.
    readonly property string buildFingerprint: ""
    property string plugin: ""
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
    // arrives, and null again for a plugin that decided to draw nothing.
    property var tree: null
    property var snapshot: null
    // Which plugin published the tree currently drawn. A press goes back to it.
    property string drawnBy: ""
    property bool connected: false
    property var plugins: []
    readonly property string status: {
        if (!link.connected) return "Waiting for Omega…"
        var name = link.plugin || link.drawnBy
        if (!name) return ""
        for (var i = 0; i < link.plugins.length; i++) {
            var plugin = link.plugins[i]
            if (plugin.plugin !== name) continue
            switch (plugin.phase) {
                case "PLUGIN_PHASE_RUNNING": return ""
                case "PLUGIN_PHASE_STARTING": return "Starting plugin…"
                case "PLUGIN_PHASE_RESTARTING": return "Restarting plugin…"
                case "PLUGIN_PHASE_STOPPED": return "Plugin stopped."
                case "PLUGIN_PHASE_FAILED": return plugin.detail || "Plugin failed to start."
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
        link.attachmentStream = 0
        link.instance = null
        link.revision = "0"
        link.presented = false
        link.instances = ({})
        link.drawnBy = ""
        link.plugins = []
        requests.disconnected(reason)
        if (!retry.running) retry.start()
    }
    // Track inbound activity to detect stale connections even without a socket close signal.
    property double lastHeard: 0
    property int nextStream: 1
    readonly property Requests requests: Requests { id: requests }

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

        if (msg.plugins) {
            link.plugins = msg.plugins.plugins || []
            return
        }

        // Only view frames may replace the rendered tree.
        if (msg.view && link.attached) link.receiveView(msg)
    }

    function receiveView(msg) {
        if (!msg.instance) return
        if (link.plugin !== "" && msg.plugin !== link.plugin) return
        if (!link.standalone && msg.surface !== link.surface) return
        var copy = Object.assign({}, link.instances)
        if (msg.destroyed) delete copy[msg.instance.id]
        else copy[msg.instance.id] = msg
        link.instances = copy
        link.snapshot = msg
        link.instance = msg.instance
        link.revision = msg.view ? msg.view.revision || "0" : "0"
        link.presented = msg.requested === 2 || msg.requested === "PRESENTATION_STATE_VISIBLE"
        link.drawnBy = msg.plugin
        link.tree = msg.view && msg.view.root ? msg.view.root : null
        link.viewUpdated(msg)
    }

    function attach() {
        if (!link.plugin || (!link.standalone && (!link.surface || !link.module))) return
        link.attachmentStream = link.allocateStream()
        var request = { features: ["RENDERER_FEATURE_INSTANCES", "RENDERER_FEATURE_SCOPED_INTERACTIONS", "RENDERER_FEATURE_LOCAL_MESSAGES", "RENDERER_FEATURE_CONTROLLED_INPUTS", "RENDERER_FEATURE_KEYBOARD_SHORTCUTS", "RENDERER_FEATURE_RESOLVED_NAVIGATION"] }
        if (link.standalone) {
            request.plugin = link.plugin
            request.features.push("RENDERER_FEATURE_WINDOWS", "RENDERER_FEATURE_OVERLAYS")
        } else {
            request.placement = { plugin: link.plugin, surface: link.surface, placement: link.module }
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

    function subscribeToPlugins() {
        link.send({
            streamId: link.allocateStream(),
            invoke: { subscribe: { topics: ["plugins"], events: [], replace: true } }
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

    property Component socketFactory: Component {
        id: socketComponent

        Socket {
            id: transport
            path: link.socketPath
            parser: SplitParser {
                onRead: function(line) {
                    if (link.socket === transport) link.onLine(line)
                }
            }
            // Clear stale views when the daemon disconnects.
            onConnectedChanged: {
                if (link.socket !== transport) return
                if (connected) {
                    retry.stop()
                    link.connected = true
                    link.subscribeToPlugins()
                    link.attach()
                } else {
                    link.disconnected()
                }
            }
            // A failed initial connection need not change the connected property.
            onError: {
                if (link.socket === transport) link.disconnected()
            }
        }
    }

    property Socket socket: null

    function openSocket() {
        if (link.socket !== null) return
        // Qt 6.4 Loader creates a context incompatible with this bound component.
        link.socket = socketComponent.createObject(link)
        // Both path and the owning reference must exist before connection signals fire.
        link.lastHeard = Date.now()
        link.socket.connected = true
    }

    function reconnect(reason) {
        link.disconnected(reason)
        retry.stop()
        link.lastHeard = Date.now()
        var previous = link.socket
        link.socket = null
        if (previous) previous.destroy()
        Qt.callLater(link.openSocket)
    }

    Component.onCompleted: link.openSocket()

    // Explicit failures retry promptly, at a bounded rate while the daemon is down.
    property Timer retry: Timer {
        id: retry
        interval: 1000
        onTriggered: link.reconnect()
    }

    // Reconnect after the heartbeat silence deadline.
    // Replace the Socket object after failure; toggling a failed socket can leave it
    // disconnected. Defer reactivation so destruction and creation cannot coalesce.
    property Timer heartbeat: Timer {
        interval: 5000
        running: true
        repeat: true
        onTriggered: {
            var expired = requests.expire(Date.now())
            if (Date.now() - link.lastHeard >= 15000 || expired) {
                link.reconnect(expired ? requests.error : "")
                return
            }
            if (link.connected && !link.attached) link.attach()
        }
    }
}
