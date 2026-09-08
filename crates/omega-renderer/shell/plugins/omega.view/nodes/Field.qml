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
    readonly property string given: Props.text(host.model, "value", "")
    readonly property bool secret: Props.flag(host.model, "secret", false)

    onGivenChanged: input.text = field.given
    Component.onCompleted: input.text = field.given

    implicitWidth: 160
    implicitHeight: Math.round(input.implicitHeight * 1.6)
    radius: 4
    color: Qt.rgba(host.foreground.r, host.foreground.g, host.foreground.b, 0.1)

    TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: 6
        anchors.rightMargin: 6
        verticalAlignment: TextInput.AlignVCenter
        clip: true
        color: field.host.ink
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
            text: Props.text(field.host.model, "placeholder", "")
            color: field.host.ink
            opacity: 0.5
            visible: input.text === ""
        }
    }
}
