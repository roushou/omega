import QtQuick
import "../Props.js" as Props

// A filled bar.
Rectangle {
    id: track
    required property var host

    implicitWidth: 48
    implicitHeight: 4
    radius: height / 2
    color: Qt.rgba(host.foreground.r, host.foreground.g, host.foreground.b, 0.2)

    Rectangle {
        width: track.width
            * Math.max(0, Math.min(1, Props.fraction(track.host.model, "value", 0)))
        height: track.height
        radius: track.radius
        color: track.host.ink
    }
}
