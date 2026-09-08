import QtQuick
import "../Props.js" as Props

// A proportion the user can drag.
//
// The drag lives here and nowhere else: a unit hears where it landed, not
// every pixel on the way, because a render round trip per frame would make
// the control lag the finger doing it. `dragging` is what keeps the published
// value from yanking the handle back mid-gesture.
Rectangle {
    id: track
    required property var host

    readonly property var bound: Props.bind(host.model, "change")
    readonly property real published:
        Math.max(0, Math.min(1, Props.fraction(host.model, "value", 0)))

    property bool dragging: false
    property real held: 0
    readonly property real shown: track.dragging ? track.held : track.published

    implicitWidth: 72
    implicitHeight: 6
    radius: height / 2
    color: Qt.rgba(host.foreground.r, host.foreground.g, host.foreground.b, 0.2)

    Rectangle {
        width: track.width * track.shown
        height: track.height
        radius: track.radius
        color: track.host.ink
    }

    MouseArea {
        anchors.fill: parent
        enabled: track.bound !== null && track.host.interactive
        cursorShape: Qt.PointingHandCursor

        function at(x) {
            return Math.max(0, Math.min(1, x / Math.max(1, track.width)))
        }

        onPressed: function (mouse) {
            track.held = at(mouse.x)
            track.dragging = true
        }
        onPositionChanged: function (mouse) {
            if (track.dragging) track.held = at(mouse.x)
        }
        onReleased: {
            if (!track.dragging) return
            track.dragging = false
            track.host.invoke(track.bound, track.held)
        }
        onCanceled: track.dragging = false
    }
}
