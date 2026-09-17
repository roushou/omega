import QtQuick
import Quickshell

// One lease per bar replica; the transport outlives any individual monitor.
QtObject {
    id: lease
    property string plugin: ""
    property string surface: ""
    property string module: ""
    property string socketPath: ""
    readonly property string key: JSON.stringify([
        socketPath || Quickshell.env("XDG_RUNTIME_DIR") + "/omega-shell.sock",
        plugin, surface, module
    ])
    property string heldKey: ""
    property bool ready: false
    property var connection: null

    function refresh() {
        if (!ready) return
        var next = plugin && surface && module ? key : ""
        if (next === heldKey) return
        connection = null
        if (heldKey) PlacementConnections.release(heldKey)
        heldKey = next
        if (heldKey) connection = PlacementConnections.acquire(heldKey)
    }
    onKeyChanged: refresh()
    Component.onCompleted: { ready = true; refresh() }
    Component.onDestruction: { if (heldKey) PlacementConnections.release(heldKey) }
}
