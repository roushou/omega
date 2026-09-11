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
Item {
    id: field
    objectName: Props.fieldName(host.model)
    required property var host

    readonly property var bound: Props.bind(host.model, "submit")
    readonly property string given: Props.fieldValue(host.model)
    property alias text: input.text
    readonly property bool secret: Props.fieldSecret(host.model)

    onGivenChanged: input.text = field.given
    Component.onCompleted: input.text = field.given

    implicitWidth: host.space(240)
    implicitHeight: labelHeight + Math.max(host.space(36), input.implicitHeight + host.space(16)) + helpHeight
    readonly property real labelHeight: fieldLabel.visible ? fieldLabel.implicitHeight + host.space(6) : 0
    readonly property real helpHeight: helpText.visible ? helpText.implicitHeight + host.space(6) : 0
    Text {
        id: helpText
        anchors.bottom: parent.bottom
        width: parent.width
        text: Props.fieldHelp(field.host.model)
        visible: text !== ""
        wrapMode: Text.Wrap
        color: field.host.ink
        opacity: 0.65
        font.family: field.host.fontFamily
        font.pixelSize: field.host.fontSize
    }
    Text {
        id: fieldLabel
        width: parent.width
        text: Props.fieldLabel(field.host.model)
        visible: text !== ""
        wrapMode: Text.Wrap
        color: field.host.ink
        font.family: field.host.fontFamily
        font.pixelSize: field.host.fontSize
    }
    Rectangle {
        anchors.fill: parent
        anchors.topMargin: field.labelHeight
        anchors.bottomMargin: field.helpHeight
        radius: host.radius
        // Through the kit's own function, so a focused field here looks like a
        // focused field anywhere else in the shell.
        color: field.host.controlFill(input.activeFocus, hover.containsMouse)

        HoverHandler { id: hover }

        TextInput {
            id: input
            anchors.fill: parent
            anchors.leftMargin: field.host.space(10)
            anchors.rightMargin: field.host.space(10)
            verticalAlignment: TextInput.AlignVCenter
            clip: true
            color: field.host.ink
            font.family: field.host.fontFamily
            font.pixelSize: field.host.fontSize
            selectByMouse: true
            enabled: field.host.interactive && (!field.host.form || field.host.form.host.interactive)
            echoMode: field.secret ? TextInput.Password : TextInput.Normal

            onAccepted: {
                if (field.host.form) { field.host.form.submit(); return }
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
}
