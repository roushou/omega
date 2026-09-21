import QtQuick
import "../Props.js" as Props
import "../Icons.js" as Icons

// Empty, loading, or failed state: a glyph, a heading, and a message.
Item {
    id: status
    required property var host

    implicitWidth: Math.max(host.space(160),
        Math.max(icon.implicitWidth, Math.max(title.implicitWidth, message.implicitWidth)))
    implicitHeight: content.implicitHeight

    Column {
        id: content
        width: status.implicitWidth
        spacing: host.space(6)

        Text {
            id: icon
            anchors.horizontalCenter: parent.horizontalCenter
            visible: text !== ""
            text: {
                var name = Props.statusIcon(host.model)
                return name === "" ? "" : (Icons.glyph(name) || name)
            }
            color: host.ink
            font.family: host.fontFamily
            font.pixelSize: host.iconSize
        }

        Text {
            id: title
            anchors.horizontalCenter: parent.horizontalCenter
            text: Props.statusTitle(host.model)
            color: host.ink
            font.family: host.fontFamily
            font.pixelSize: host.typeSize("title", host.fontSize)
            font.bold: true
        }

        Text {
            id: message
            anchors.horizontalCenter: parent.horizontalCenter
            visible: text !== ""
            text: Props.statusMessage(host.model)
            color: host.ink
            opacity: 0.7
            font.family: host.fontFamily
            font.pixelSize: host.fontSize
        }
    }
}
