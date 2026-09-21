import QtQuick
import "../Props.js" as Props

// A single key or chord drawn as a keycap, for shortcuts and hints.
Rectangle {
    id: keycap
    required property var host

    implicitWidth: label.implicitWidth + host.space(10)
    implicitHeight: label.implicitHeight + host.space(4)
    radius: host.radius
    color: host.idleFill
    border.width: host.space(1)
    border.color: host.rule

    Text {
        id: label
        anchors.centerIn: parent
        text: Props.keycapLabel(host.model)
        color: host.ink
        font.family: host.fontFamily
        font.pixelSize: host.captionSize
    }
}
