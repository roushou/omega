import QtQuick
import QtQuick.Templates as T
import "../Props.js" as Props
import "../Icons.js" as Icons

// Button with command or local-message activation and pending feedback.
T.Control {
    id: slot
    required property var host

    readonly property var bound: Props.bind(host.model, "press")
    readonly property bool pressable: slot.bound !== null && slot.host.interactive
    readonly property bool hot: press.containsMouse && slot.pressable

    // A `Group` segment's paddings, so both controls share a height without
    // either naming a number.
    implicitWidth: content.implicitWidth + slot.host.space(24)
    implicitHeight: Math.max(slot.host.space(36), content.implicitHeight + slot.host.space(16))

    focusPolicy: Qt.StrongFocus
    enabled: slot.bound !== null
    Keys.onReturnPressed: if (slot.pressable) slot.host.invoke("press", undefined)
    Keys.onEnterPressed: if (slot.pressable) slot.host.invoke("press", undefined)
    Keys.onSpacePressed: if (slot.pressable) slot.host.invoke("press", undefined)

    background: Rectangle {
        radius: slot.host.radius
        color: slot.hot ? slot.host.hoverFill : Props.buttonFlat(slot.host.model) ? "transparent"
            : Props.emphasis(slot.host.model) === "primary" ? slot.host.chosenFill : slot.host.idleFill
        border.width: !Props.buttonFlat(slot.host.model) || slot.visualFocus ? slot.host.space(1) : 0
        border.color: slot.visualFocus ? slot.host.ink : slot.host.rule
        Behavior on color {
            ColorAnimation { duration: slot.host.theme.motion ? 120 : 0; easing.type: Easing.OutCubic }
        }
    }

    Row {
        id: content
        anchors.centerIn: parent
        spacing: slot.host.space(6)

        Text {
            readonly property string name: Props.buttonIcon(slot.host.model)
            visible: name !== ""
            text: Icons.glyph(name) || name
            color: label.color
            font.family: slot.host.fontFamily
            font.pixelSize: slot.host.fontSize
            anchors.verticalCenter: parent.verticalCenter
        }

        Text {
            id: label
            text: Props.buttonLabel(slot.host.model)
            color: slot.hot ? slot.host.hoverInk : slot.host.ink
            font.family: slot.host.fontFamily
            font.pixelSize: slot.host.fontSize
            font.bold: Props.bold(slot.host.model)
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        id: press
        anchors.fill: parent
        hoverEnabled: true
        enabled: slot.pressable
        cursorShape: Qt.PointingHandCursor
        // Button activation supplies only its retained binding.
        onClicked: {
            slot.forceActiveFocus(Qt.MouseFocusReason)
            // Forwarded bar clicks can arrive without a new focus event.
            slot.focusReason = Qt.MouseFocusReason
            slot.host.invoke("press", undefined)
        }
    }
}
