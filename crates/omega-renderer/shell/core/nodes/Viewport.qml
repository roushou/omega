import QtQuick
import "../Props.js" as Props

// A clipped, transformed view of its content. The renderer owns the live
// transform so gestures are immediate; the surface observes them as events and
// commands a new transform by changing `revision`.
Item {
    id: viewport
    required property var host
    clip: true

    // Mirrors the surface's Zoom range. Live gestures clamp here; the surface
    // clamps when it commands a transform.
    readonly property real minZoom: 0.25
    readonly property real maxZoom: 8

    readonly property string fit: Props.viewportFit(host.model)
    readonly property real baseScale: {
        var cw = content.implicitWidth
        var ch = content.implicitHeight
        if (cw <= 0 || ch <= 0 || viewport.width <= 0 || viewport.height <= 0) return 1
        switch (fit) {
            case "cover": return Math.max(viewport.width / cw, viewport.height / ch)
            case "none": return 1
            default: return Math.min(viewport.width / cw, viewport.height / ch)
        }
    }
    readonly property real scale: baseScale * zoom

    property real zoom: 1
    property real offsetX: 0
    property real offsetY: 0

    readonly property int publishedRevision:
        host && host.model ? Props.viewportRevision(host.model) : 0
    onPublishedRevisionChanged: viewport.adopt()
    Component.onCompleted: viewport.adopt()

    // Replace the live transform with the surface's command. Only a revision
    // change does this, so a running gesture is never overwritten by a render.
    function adopt() {
        zoom = viewport.clamp(Props.viewportZoom(host.model))
        offsetX = Props.viewportOffset_x(host.model)
        offsetY = Props.viewportOffset_y(host.model)
    }

    function clamp(value) {
        return Math.max(viewport.minZoom, Math.min(viewport.maxZoom, value))
    }

    // The content point under a viewport position, at the current transform.
    function contentAt(px, py) {
        var current = viewport.scale
        if (current <= 0) return Qt.point(0, 0)
        var left = viewport.width / 2 + viewport.offsetX - content.implicitWidth * current / 2
        var top = viewport.height / 2 + viewport.offsetY - content.implicitHeight * current / 2
        return Qt.point((px - left) / current, (py - top) / current)
    }

    // Scale to `next`, keeping the content point under (ax, ay) fixed.
    function zoomTo(next, ax, ay) {
        var anchor = viewport.contentAt(ax, ay)
        zoom = viewport.clamp(next)
        var scaled = viewport.scale
        offsetX = ax - anchor.x * scaled - viewport.width / 2 + content.implicitWidth * scaled / 2
        offsetY = ay - anchor.y * scaled - viewport.height / 2 + content.implicitHeight * scaled / 2
    }

    function report(event, point, dx, dy) {
        viewport.host.invoke(event, {
            "zoom": viewport.zoom,
            "offset_x": viewport.offsetX,
            "offset_y": viewport.offsetY,
            "x": point ? point.x : 0,
            "y": point ? point.y : 0,
            "dx": dx,
            "dy": dy
        })
    }

    // The content is centred in the viewport and then panned. `offset` is a
    // viewport-pixel translation of the content's centre from that centre.
    Item {
        id: layer
        objectName: "viewportLayer"
        width: content.implicitWidth
        height: content.implicitHeight
        transformOrigin: Item.TopLeft
        scale: viewport.scale
        x: viewport.width / 2 + viewport.offsetX - width * viewport.scale / 2
        y: viewport.height / 2 + viewport.offsetY - height * viewport.scale / 2

        // A viewport presents one canvas. Load it once and update the retained
        // ViewNode's model in place, so a surface render never reloads the image.
        Loader {
            id: content
            readonly property var canvas: {
                var children = Props.children(viewport.host.model)
                return children && children.length > 0 ? children[0] : null
            }
            onCanvasChanged: if (content.item) content.item.model = content.canvas
            Component.onCompleted: if (content.canvas) setSource("../ViewNode.qml", {
                "model": content.canvas,
                "session": Qt.binding(function() { return viewport.host.session }),
                "theme": Qt.binding(function() { return viewport.host.theme }),
                "assets": Qt.binding(function() { return viewport.host.assets }),
                "foreground": Qt.binding(function() { return viewport.host.ink })
            })
        }
    }

    WheelHandler {
        id: wheel
        enabled: viewport.host.interactive && Props.bind(viewport.host.model, "wheel") !== null
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onWheel: event => {
            var point = wheel.point.position
            viewport.zoomTo(
                viewport.zoom * Math.pow(1.0015, event.angleDelta.y),
                point.x, point.y)
            viewport.report("wheel", point, 0, 0)
        }
    }

    DragHandler {
        id: drag
        enabled: viewport.host.interactive && Props.bind(viewport.host.model, "drag") !== null
        property real heldX: 0
        property real heldY: 0
        onActiveChanged: {
            if (drag.active) {
                drag.heldX = 0
                drag.heldY = 0
            } else {
                // Panning is local; report the settled transform once so the
                // surface can track it without a render per mouse move.
                viewport.report("drag", null, 0, 0)
            }
        }
        onTranslationChanged: {
            if (!drag.active) return
            var dx = drag.translation.x - drag.heldX
            var dy = drag.translation.y - drag.heldY
            drag.heldX = drag.translation.x
            drag.heldY = drag.translation.y
            viewport.offsetX += dx
            viewport.offsetY += dy
        }
    }

    PinchHandler {
        id: pinch
        enabled: viewport.host.interactive && Props.bind(viewport.host.model, "pinch") !== null
        property real held: 1
        onActiveChanged: {
            if (pinch.active) pinch.held = 1
        }
        onScaleChanged: {
            if (!pinch.active) return
            var factor = pinch.scale / pinch.held
            pinch.held = pinch.scale
            var point = pinch.centroid.position
            viewport.zoomTo(viewport.zoom * factor, point.x, point.y)
            viewport.report("pinch", point, 0, 0)
        }
    }
}
