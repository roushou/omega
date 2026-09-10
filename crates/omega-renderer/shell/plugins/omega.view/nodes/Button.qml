import QtQuick
import "../Props.js" as Props

// Something to press.
//
// The binding names one of the drawing unit's own commands, so a press reaches
// that unit and no other. Drawn as a filled slot at the theme's state alphas,
// like a `Group`'s segments: the idle fill is 4% and needs the border to be
// found at all, and without one a button is a word that happens to be
// clickable.
Rectangle {
    id: slot
    required property var host

    readonly property var bound: Props.bind(host.model, "press")
    readonly property bool pressable: slot.bound !== null && slot.host.interactive
    readonly property bool hot: press.containsMouse && slot.pressable

    // A `Group` segment's paddings, so both controls share a height without
    // either naming a number.
    implicitWidth: label.implicitWidth + slot.host.space(16)
    implicitHeight: label.implicitHeight + slot.host.space(8)

    radius: slot.host.radius
    color: slot.hot ? slot.host.hoverFill : slot.host.idleFill
    border.width: slot.host.space(1)
    border.color: slot.host.rule

    Behavior on color {
        ColorAnimation { duration: 120; easing.type: Easing.OutCubic }
    }

    Text {
        id: label
        anchors.centerIn: parent

        text: Props.buttonLabel(slot.host.model)
        color: slot.hot ? slot.host.hoverInk : slot.host.ink
        font.family: slot.host.fontFamily
        font.pixelSize: slot.host.fontSize
        font.bold: Props.bold(slot.host.model)
        verticalAlignment: Text.AlignVCenter
    }

    MouseArea {
        id: press
        anchors.fill: parent
        hoverEnabled: true
        enabled: slot.pressable
        cursorShape: Qt.PointingHandCursor
        // A press carries nothing of its own: the binding is the whole
        // message.
        onClicked: slot.host.invoke(slot.bound, undefined)
    }
}
