import QtQuick
import "../core"

Rectangle {
    id: viewport
    required property Theme theme
    required property Assets assets
    required property var session
    property var view: null
    property var epoch: "0"
    color: theme.background
    clip: true
    onEpochChanged: {
        content.active = false
        Qt.callLater(function() { content.active = true })
    }
    Loader {
        id: content
        anchors.fill: parent
        anchors.margins: 16
        sourceComponent: Component {
            ViewNode {
                model: viewport.view ? viewport.view.root || null : null
                theme: viewport.theme
                assets: viewport.assets
                session: viewport.session
            }
        }
    }
}
