import QtQuick
import "../Props.js" as Props

// Children in a scrollable viewport. The ViewNode wrapper bounds the height
// from the shared height prop; this reports its natural content size.
Flickable {
    id: scroll
    required property var host

    clip: true
    contentWidth: content.implicitWidth
    contentHeight: content.implicitHeight
    implicitWidth: content.implicitWidth
    implicitHeight: content.implicitHeight

    Column {
        id: content
        width: scroll.width

        Repeater {
            model: Props.children(scroll.host.model)
            delegate: child
        }
    }

    Component {
        id: child
        Loader {
            id: cell
            required property var modelData
            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": cell.modelData,
                "session": Qt.binding(function() { return scroll.host.session }),
                "theme": Qt.binding(function() { return scroll.host.theme }),
                "assets": Qt.binding(function() { return scroll.host.assets }),
                "foreground": Qt.binding(function() { return scroll.host.ink })
            })
        }
    }
}
