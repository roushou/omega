import QtQuick
import "../Props.js" as Props

// A series, drawn small.
//
// A filled line rather than bars: at this size the eye reads a shape, and the
// shape of a latency history is what somebody is looking for. The newest
// point is the right-hand edge, which is where an eye goes for "now".
Canvas {
    id: graph
    required property var host

    readonly property var points: Props.graphPoints(host.model)
    readonly property real low: Props.graphLow(host.model)
    readonly property real high: Props.graphHigh(host.model)

    // Given a range, use it. Given none — or one that is not a range — scale
    // to the data, which is right for a latency and wrong for a percentage.
    // The unit is the one that knows which it has.
    readonly property bool pinned: graph.high > graph.low

    implicitWidth: 72
    implicitHeight: 20

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
        // One point is not a line. Nothing to draw is not a reason to draw
        // something wrong.
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

        // A flat series has no range to scale against. Draw it down the
        // middle rather than divide by nothing.
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
        ctx.fillStyle = Qt.rgba(graph.host.ink.r, graph.host.ink.g, graph.host.ink.b, 0.18)
        ctx.fill()
        ctx.restore()

        ctx.strokeStyle = graph.host.ink
        ctx.lineWidth = 1
        ctx.stroke()
    }
}
