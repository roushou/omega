import QtQuick
import qs.Commons
import "core"

Theme {
    foreground: Color.foreground
    background: Color.background
    accent: Color.accent
    urgent: Color.urgent
    muted: Color.muted
    font: Style.font
    cornerRadius: Style.cornerRadius

    function space(value) { return Style.space(value) }
    function normalFillFor(base, accent, urgent) { return Style.normalFillFor(base, accent, urgent) }
    function hoverFillFor(base, accent, urgent) { return Style.hoverFillFor(base, accent, urgent) }
    function selectedFillFor(base, accent, urgent) { return Style.selectedFillFor(base, accent, urgent) }
    function hoverStateColor(base, accent, urgent) { return Style.hoverStateColor(base, accent, urgent) }
    function controlFill(focused, hot, base, accent) { return Style.controlFill(focused, hot, base, accent) }
}
