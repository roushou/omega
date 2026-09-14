import QtQuick
import "../Props.js" as Props

// Filled line graph with samples ordered oldest to newest.
Canvas {
    id: graph
    required property var host

    readonly property var points: Props.graphPoints(host.model)
    readonly property real low: Props.graphLow(host.model)
    readonly property real high: Props.graphHigh(host.model)

    // Use explicit valid bounds or derive the scale from samples.
    readonly property bool pinned: graph.high > graph.low

    implicitWidth: host.space(72)
    implicitHeight: host.space(20)

    // Repaint whenever any of it moves. A canvas does not redraw itself when
    // the data behind it changes.
    onPointsChanged: graph.requestPaint()
    onLowChanged: graph.requestPaint()
    onHighChanged: graph.requestPaint()
    onWidthChanged: graph.requestPaint()
    onHeightChanged: graph.requestPaint()

    onPaint: {
        var ctx = graph.getContext("2d")
        ctx.reset()

        var count = graph.points.length
        // At least two points are required to draw a line.
        if (count < 2) return

        var lowest = graph.low
        var highest = graph.high
        if (!graph.pinned) {
            lowest = graph.points[0]
            highest = graph.points[0]
            for (var s = 1; s < count; s++) {
                if (graph.points[s] < lowest) lowest = graph.points[s]
                if (graph.points[s] > highest) highest = graph.points[s]
            }
        }

        // Place constant-valued series at mid-height to avoid division by zero.
        var span = highest - lowest
        var flat = span <= 0

        function at(index) {
            var x = (index / (count - 1)) * graph.width
            var value = graph.points[index]
            var fraction = flat ? 0.5 : (value - lowest) / span
            fraction = Math.max(0, Math.min(1, fraction))
            // Canvas y grows downward and a graph does not.
            return { x: x, y: graph.height - fraction * graph.height }
        }

        ctx.beginPath()
        var start = at(0)
        ctx.moveTo(start.x, start.y)
        for (var i = 1; i < count; i++) {
            var point = at(i)
            ctx.lineTo(point.x, point.y)
        }

        // The fill closes down to the baseline; the stroke must not, or the
        // bottom edge is drawn as part of the line.
        ctx.save()
        ctx.lineTo(graph.width, graph.height)
        ctx.lineTo(0, graph.height)
        ctx.closePath()
        // The area under the line, at the same weight as any other filled
        // chrome in the shell.
        ctx.fillStyle = graph.host.chosenFill
        ctx.fill()
        ctx.restore()

        ctx.strokeStyle = graph.host.ink
        ctx.lineWidth = 1
        ctx.stroke()
    }
}
