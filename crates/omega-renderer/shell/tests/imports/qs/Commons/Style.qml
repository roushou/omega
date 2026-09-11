pragma Singleton
import QtQuick
// Host theme values are fixed here; tests exercise renderer state, not Omarchy.
QtObject {
    readonly property var font: ({family:"sans-serif",body:14,caption:12,icon:14,subtitle:16,title:18,heading:20,display:24})
    readonly property int cornerRadius: 3
    function space(value) { return value }
    function normalFillFor(base, accent, urgent) { return base }
    function hoverFillFor(base, accent, urgent) { return base }
    function selectedFillFor(base, accent, urgent) { return accent }
    function hoverStateColor(base, accent, urgent) { return base }
    function controlFill(focused, hot, base, accent) { return base }
}
