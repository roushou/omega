import QtQuick

// Hosts may bind these tokens to their desktop theme at runtime.
QtObject {
    property bool motion: true
    property color foreground: "#e4e4e7"
    property color background: "#18181b"
    property color accent: "#93c5fd"
    property color urgent: "#fda4af"
    property color muted: "#a1a1aa"
    property real scale: 1
    property var font: ({family: "sans-serif", body: 14, caption: 12,
        icon: 16, subtitle: 16, title: 18, heading: 22, display: 28})
    property int cornerRadius: 6

    function space(value) { return Math.round(value * scale) }
    function alpha(color, amount) { return Qt.rgba(color.r, color.g, color.b, color.a * amount) }
    function normalFillFor(base, accent, urgent) { return alpha(base, 0.04) }
    function hoverFillFor(base, accent, urgent) { return alpha(base, 0.10) }
    function selectedFillFor(base, accent, urgent) { return alpha(base, 0.16) }
    function hoverStateColor(base, accent, urgent) { return base }
    function controlFill(focused, hot, base, accent) {
        return focused ? alpha(accent, 0.16) : alpha(base, hot ? 0.10 : 0.04)
    }
}
