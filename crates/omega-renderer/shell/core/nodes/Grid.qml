import QtQuick
import "../Props.js" as Props

// Grid layout with shared column alignment.
Grid {
    id: grid
    required property var host

    columns: Math.max(1, Props.gridColumns(host.model))
    spacing: host.space(Props.gridGap(host.model))

    Repeater {
        model: Props.children(grid.host.model)
        delegate: cell
    }

    // Load by URL so modelData remains a delegate property.
    Component {
        id: cell
        Loader {
            id: child
            required property var modelData

            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": child.modelData,
                "session": Qt.binding(function() { return grid.host.session }),
"theme": Qt.binding(function() { return grid.host.theme }),
"assets": Qt.binding(function() { return grid.host.assets }),
                "foreground": Qt.binding(function() { return grid.host.ink })
            })
        }
    }
}
