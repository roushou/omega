import QtQuick

QtObject {
    id: panel
    required property var connection
    readonly property bool opened: connection !== null && connection.connected && connection.instance !== null && connection.presented
    readonly property string requestKey: "presentation"
    property bool pending: false
    property bool acknowledged: false
    property bool seen: false
    property bool target: false
    property var queued: null
    property var hosts: []
    property var owner: null

    function registerHost(host) {
        if (hosts.indexOf(host) !== -1) return
        hosts = hosts.concat([host])
        if (opened && !owner && !pending) owner = host
    }

    function releaseHost(host) {
        hosts = hosts.filter(candidate => candidate !== host)
        if (owner !== host) return
        setOpen(false)
        owner = null
    }
    onConnectionChanged: {
        pending = false
        queued = null
    }

    // Only daemon intent drives visibility; observations never issue commands.
    onOpenedChanged: {
        if (opened && !owner && hosts.length > 0 && !pending) owner = hosts[0]
        if (connection && connection.instance)
            connection.report(connection.instance, opened ? "PRESENTATION_STATE_VISIBLE" : "PRESENTATION_STATE_HIDDEN")
    }

    function toggle(host) {
        // Clicking this placement on another output transfers its native popup.
        if (host && owner !== host) { setOpen(true, host); return }
        setOpen(!(queued !== null ? queued : pending ? target : opened), host)
    }

    function setOpen(shown, host) {
        if (!connection) return
        if (shown && host) owner = host
        if (pending) { queued = shown; return }
        if (shown === opened) return
        target = shown
        acknowledged = false
        seen = false
        pending = true
        if (!connection.change(shown ? "PRESENTATION_ACTION_PRESENT" : "PRESENTATION_ACTION_HIDE", requestKey)) {
            pending = false
            queued = null
        }
    }

    function finish() {
        if (!pending || !acknowledged || !seen) return
        pending = false
        var next = queued
        queued = null
        if (next !== null && next !== target) setOpen(next)
    }

    property Connections snapshots: Connections {
        target: panel.connection
        function onViewUpdated(snapshot) {
            if (panel.pending && panel.connection.presented === panel.target) {
                panel.seen = true
                panel.finish()
            }
        }
        function onConnectedChanged() {
            if (!panel.connection.connected) {
                panel.pending = false
                panel.queued = null
            }
        }
    }

    // Results and snapshots use separate streams and may arrive in either order.
    property Connections completion: Connections {
        target: panel.connection ? panel.connection.requests : null
        function onSettled(key, success) {
            if (key !== panel.requestKey || !panel.pending) return
            if (!success) {
                panel.pending = false
                panel.queued = null
                return
            }
            panel.acknowledged = true
            panel.finish()
        }
    }

    property Timer deadline: Timer {
        interval: 30000
        running: panel.pending
        onTriggered: panel.connection.reconnect("Presentation timed out; its outcome is unknown.")
    }
}
