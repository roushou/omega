import QtQuick
import ".."
import "../Props.js" as Props

// Children in a scrollable viewport. The ViewNode wrapper bounds the height
// from the shared height prop; this reports its natural content size.
Flickable {
    id: scroll
    required property var host

    readonly property var wanted: Props.children(scroll.host.model)
    KeyedChildren { id: childRows }
    onWantedChanged: childRows.reconcile(wanted)
    Component.onCompleted: childRows.reconcile(wanted)

    clip: true
    contentWidth: content.implicitWidth
    contentHeight: content.implicitHeight
    implicitWidth: content.implicitWidth
    implicitHeight: content.implicitHeight

    Column {
        id: content
        width: scroll.width

        Repeater {
            model: childRows
            delegate: child
        }
    }

    Component {
        id: child
        Loader {
            id: cell
            required property var node
            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": Qt.binding(function() { return cell.node }),
                "session": Qt.binding(function() { return scroll.host.session }),
                "navigation": Qt.binding(function() { return scroll.host.navigation || null }),
                "theme": Qt.binding(function() { return scroll.host.theme }),
                "assets": Qt.binding(function() { return scroll.host.assets }),
                "foreground": Qt.binding(function() { return scroll.host.ink })
            })
        }
    }
}
