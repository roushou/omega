import QtQuick
import "../Props.js" as Props

// Something to press.
//
// The binding it carries names one of the drawing unit's own commands, so a
// press reaches that unit and no other, and carries the arguments the unit
// asked to have handed back.
//
// Drawn as a filled slot at the theme's own state alphas, the way a `Group`'s
// segments are: a button that was only its label was a word that happened to
// be clickable, which is not something a panel's footer can be read as an
// action. The fill is faint by design and the border is what finds it — the
// kit's idle fill is 4% and was never meant to stand on its own.
Rectangle {
    id: slot
    required property var host

    readonly property var bound: Props.bind(host.model, "press")
    readonly property bool pressable: slot.bound !== null && slot.host.interactive
    readonly property bool hot: press.containsMouse && slot.pressable

    // The paddings a `Group` segment uses, so the two controls a panel is
    // made of are the same height without either naming a number.
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
