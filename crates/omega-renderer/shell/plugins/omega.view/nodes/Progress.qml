import QtQuick
import "../Props.js" as Props

// A filled bar.
Rectangle {
    id: track
    required property var host

    implicitWidth: host.space(48)
    implicitHeight: host.space(4)
    radius: host.pill(height)
    color: host.trackFill

    Rectangle {
        width: track.width
            * Math.max(0, Math.min(1, Props.progressValue(track.host.model)))
        height: track.height
        radius: track.radius
        color: track.host.ink
    }
}
