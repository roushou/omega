import QtQuick
import QtTest
import "../core/nodes" as Nodes
import "../core" as Renderer

// The viewport reports gestures and applies the transform the surface publishes.
TestCase {
    id: test
    name: "Viewport"
    when: windowShown
    visible: true
    width: 400
    height: 400

    property var submitted: null
    property string submittedEvent: ""

    Renderer.Requests { id: requestState }
    QtObject {
        id: testConnection
        readonly property var requests: requestState
    }

    QtObject {
        id: host
        property Renderer.Theme theme: Renderer.Theme {}
        property Renderer.Assets assets: Renderer.Assets {}
        property var session: testConnection
        property bool pending: false
        property bool interactive: !pending
        property color ink: "white"
        property color foreground: "white"
        property var model: ({
            type: "viewport",
            props: {
                zoom: { doubleValue: 2.0 },
                offset_x: { doubleValue: 30.0 },
                offset_y: { doubleValue: -10.0 },
                fit: { stringValue: "contain" }
            },
            events: {
                wheel: { command: "wheel" },
                drag: { command: "drag" },
                pinch: { command: "pinch" }
            },
            children: [
                { type: "spacer", props: { width: { intValue: "100" }, height: { intValue: "50" } } }
            ]
        })
        function space(value) { return value }
        function invoke(event, value) {
            test.submitted = value
            test.submittedEvent = event
        }
    }

    Nodes.Viewport { id: viewport; host: host; width: 300; height: 200 }

    function test_published_transform_positions_the_content() {
        wait(100)
        // A gesture may have run first and owns the live transform; re-adopt the
        // published command explicitly.
        viewport.adopt()
        var layer = findChild(viewport, "viewportLayer")
        verify(layer !== null)
        // base = min(300/100, 200/50) = 3, so scale = 3 * 2 = 6.
        fuzzyCompare(layer.scale, 6, 0.001)
        fuzzyCompare(layer.x, 150 + 30 - 100 * 6 / 2, 0.001)
        fuzzyCompare(layer.y, 100 - 10 - 50 * 6 / 2, 0.001)
    }

    function test_wheel_reports_a_zoom_factor_and_pointer() {
        failOnWarning(/.*/)
        test.submitted = null
        mouseWheel(viewport, 120, 80, 0, 120)
        verify(test.submitted !== null)
        verify(test.submitted.zoom > 1)
        compare(test.submittedEvent, "wheel")
        fuzzyCompare(test.submitted.x, 120, 0.001)
        fuzzyCompare(test.submitted.y, 80, 0.001)
    }

    function test_drag_reports_settled_transform() {
        test.submitted = null
        // The published initial offset is (30, -10); a drag of (30, -10) adds to it.
        mouseDrag(viewport, 100, 100, 30, -10)
        verify(test.submitted !== null)
        compare(test.submittedEvent, "drag")
        verify(test.submitted.offset_x > 30)
        verify(test.submitted.offset_y < -10)
    }
}
