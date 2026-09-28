import QtQuick
import ".."
import "../Props.js" as Props

// Grid layout with shared column alignment.
Grid {
    id: grid
    required property var host

    readonly property var wanted: Props.children(grid.host.model)
    KeyedChildren { id: childRows }
    onWantedChanged: childRows.reconcile(wanted)
    Component.onCompleted: childRows.reconcile(wanted)

    columns: Math.max(1, Props.gridColumns(host.model))
    spacing: host.space(Props.gridGap(host.model))

    Repeater {
        model: childRows
        delegate: cell
    }

    // URL loading permits recursive child nodes.
    Component {
        id: cell
        Loader {
            id: child
            required property var node

            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": Qt.binding(function() { return child.node }),
                "session": Qt.binding(function() { return grid.host.session }),
                "navigation": Qt.binding(function() { return grid.host.navigation || null }),
                "theme": Qt.binding(function() { return grid.host.theme }),
                "assets": Qt.binding(function() { return grid.host.assets }),
                "foreground": Qt.binding(function() { return grid.host.ink })
            })
        }
    }
}
