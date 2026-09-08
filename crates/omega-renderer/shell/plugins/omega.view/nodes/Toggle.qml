import QtQuick
import "../Props.js" as Props

// Something with two states.
//
// It flips as soon as it is pressed rather than waiting to be told. The round
// trip is short, but not so short that a switch which hesitates reads as a
// switch that did not work — and the next render says what is actually true,
// so an optimistic flip that the unit refuses corrects itself.
Rectangle {
    id: swtch
    required property var host

    readonly property var bound: Props.bind(host.model, "change")
    readonly property bool published: Props.flag(host.model, "on", false)

    // Null until pressed, then the state we are showing until the unit
    // publishes one of its own.
    property var optimistic: null
    readonly property bool checked:
        swtch.optimistic === null ? swtch.published : swtch.optimistic

    // The unit answered; stop second-guessing it.
    onPublishedChanged: swtch.optimistic = null

    implicitWidth: 28
    implicitHeight: 16
    radius: height / 2
    color: swtch.checked
        ? swtch.host.ink
        : Qt.rgba(host.foreground.r, host.foreground.g, host.foreground.b, 0.2)

    Rectangle {
        width: swtch.height - 4
        height: width
        radius: width / 2
        y: 2
        x: swtch.checked ? swtch.width - width - 2 : 2
        color: swtch.host.foreground

        Behavior on x {
            NumberAnimation { duration: 90; easing.type: Easing.OutCubic }
        }
    }

    MouseArea {
        anchors.fill: parent
        enabled: swtch.bound !== null && swtch.host.interactive
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            var next = !swtch.checked
            swtch.optimistic = next
            swtch.host.invoke(swtch.bound, next)
        }
    }
}
