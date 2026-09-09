import QtQuick
import "../Props.js" as Props

// Something to type into.
//
// The buffer lives here. A half-typed passphrase is not a fact about the
// machine, and telling the unit every keystroke would put a socket round trip
// in the path of each character.
//
// A unit that gives the node a `value` takes the buffer over — that is how a
// field gets cleared after being acted on. Without one, what was typed
// survives a re-render, which is what stops a list refreshing underneath
// somebody mid-passphrase.
Rectangle {
    id: field
    required property var host

    readonly property var bound: Props.bind(host.model, "submit")
    readonly property string given: Props.fieldValue(host.model)
    readonly property bool secret: Props.fieldSecret(host.model)

    onGivenChanged: input.text = field.given
    Component.onCompleted: input.text = field.given

    implicitWidth: host.space(160)
    implicitHeight: Math.round(input.implicitHeight * 1.6)
    radius: host.radius
    // Through the kit's own function, so a focused field here looks like a
    // focused field anywhere else in the shell.
    color: field.host.controlFill(input.activeFocus, hover.containsMouse)

    HoverHandler { id: hover }

    TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: field.host.space(6)
        anchors.rightMargin: field.host.space(6)
        verticalAlignment: TextInput.AlignVCenter
        clip: true
        color: field.host.ink
        font.family: field.host.fontFamily
        font.pixelSize: field.host.fontSize
        selectByMouse: true
        enabled: field.host.interactive
        echoMode: field.secret ? TextInput.Password : TextInput.Normal

        onAccepted: {
            if (field.bound === null) return
            field.host.invoke(field.bound, input.text)
        }

        Text {
            anchors.fill: parent
            verticalAlignment: Text.AlignVCenter
            text: Props.fieldPlaceholder(field.host.model)
            color: Qt.darker(field.host.ink, 1.4)
            font.family: field.host.fontFamily
            font.pixelSize: field.host.fontSize
            visible: input.text === ""
        }
    }
}
