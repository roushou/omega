import QtQuick
import "../Props.js" as Props

// A small count pill. Hides at zero when asked; tone carries its meaning.
Rectangle {
    id: badge
    required property var host

    readonly property int count: Props.badgeCount(host.model)
    visible: !(Props.badgeHidden_when_zero(host.model) && badge.count === 0)

    implicitWidth: label.implicitWidth + host.space(10)
    implicitHeight: Math.max(host.space(16), label.implicitHeight + host.space(2))
    radius: host.pill(height)
    color: host.chosenFill

    Text {
        id: label
        anchors.centerIn: parent
        text: String(badge.count)
        color: host.ink
        font.family: host.fontFamily
        font.pixelSize: host.captionSize
    }
}
