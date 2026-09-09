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

    readonly property bool hot:
        press.containsMouse && label.bound !== null && label.host.interactive

    text: Props.buttonLabel(host.model)
    color: label.hot ? label.host.hoverInk : label.host.ink
    font.family: host.fontFamily
    font.pixelSize: host.fontSize
    font.bold: Props.bold(host.model)
    verticalAlignment: Text.AlignVCenter

    MouseArea {
        id: press
        anchors.fill: parent
        hoverEnabled: true
        enabled: label.bound !== null && label.host.interactive
        cursorShape: Qt.PointingHandCursor
        // A press carries nothing of its own: the binding is the whole
        // message.
        onClicked: label.host.invoke(label.bound, undefined)
    }
}
