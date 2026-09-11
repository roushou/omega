import QtQuick
import "../Props.js" as Props

// A proportion the user can drag.
//
// The drag lives here and nowhere else: a unit hears where it landed, not
// every pixel on the way, because a render round trip per frame would make
// the control lag the finger doing it. `dragging` is what keeps the published
// value from yanking the handle back mid-gesture.
Item {
    id: track
    required property var host

    readonly property var bound: Props.bind(host.model, "change")
    readonly property real published:
        Math.max(0, Math.min(1, Props.sliderValue(host.model)))

    property bool dragging: false
    property real held: 0
    readonly property real shown: track.dragging || track.host.pending ? track.held : track.published

    implicitWidth: host.space(240)
    implicitHeight: host.space(32)
    activeFocusOnTab: true

    Rectangle {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        height: track.host.space(6)
        radius: track.host.pill(height)
        color: track.host.trackFill
        Rectangle {
            width: parent.width * track.shown
            height: parent.height
            radius: parent.radius
            color: track.host.ink
        }
    }
    Rectangle {
        width: track.host.space(track.activeFocus || track.dragging ? 18 : 14)
        height: width
        radius: track.host.pill(width)
        x: Math.max(0, Math.min(track.width - width, track.width * track.shown - width / 2))
        anchors.verticalCenter: parent.verticalCenter
        color: track.host.ink
    }

    function adjust(delta) {
        if (track.bound === null || !track.host.interactive) return
        track.held = Math.max(0, Math.min(1, track.published + delta))
        track.host.invoke(track.bound, track.held)
    }
    Keys.onLeftPressed: adjust(-0.05)
    Keys.onRightPressed: adjust(0.05)

    MouseArea {
        anchors.fill: parent
        enabled: track.bound !== null && track.host.interactive
        cursorShape: Qt.PointingHandCursor

        function at(x) {
            return Math.max(0, Math.min(1, x / Math.max(1, track.width)))
        }

        onPressed: function (mouse) {
            track.forceActiveFocus()
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
