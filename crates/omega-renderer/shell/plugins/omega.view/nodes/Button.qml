import QtQuick
import "../Props.js" as Props

// Something to press.
//
// The binding it carries names one of the drawing unit's own commands, so a
// press reaches that unit and no other, and carries the arguments the unit
// asked to have handed back.
Text {
    id: label
    required property var host

    readonly property var bound: Props.bind(host.model, "press")

    text: Props.text(host.model, "label", "")
    color: host.ink
    font.bold: Props.flag(host.model, "bold", false)
    verticalAlignment: Text.AlignVCenter

    MouseArea {
        anchors.fill: parent
        enabled: label.bound !== null && label.host.interactive
        cursorShape: Qt.PointingHandCursor
        // A press carries nothing of its own: the binding is the whole
        // message.
        onClicked: label.host.invoke(label.bound, undefined)
    }
}
