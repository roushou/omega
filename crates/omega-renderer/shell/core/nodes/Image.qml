import QtQuick
import "../Props.js" as Props

// Local-file and data-URI images. Load asynchronously to avoid blocking rendering.
Image {
    id: picture
    required property var host

    readonly property string fit: Props.imageFit(host.model)
    // "none" keeps the decoded natural size; every other mode fills the box
    // the Node was allocated, so layout decides the picture's bounds.
    readonly property bool sized: fit !== "none"
    readonly property int innerWidth: Math.max(0, host.width - host.padding * 2)
    readonly property int innerHeight: Math.max(0, host.height - host.padding * 2)

    onStatusChanged: host.assets.imageState(picture, status)
    Component.onDestruction: host.assets.imageState(picture, Image.Null)

    source: host.assets.resolve(Props.imageSource(host.model))
    visible: source !== "" && status === Image.Ready
    asynchronous: true
    cache: true
    // Honor embedded orientation before fitting, so a rotated photo is not
    // drawn sideways or fitted against swapped dimensions.
    autoTransform: true
    fillMode: {
        switch (fit) {
            case "cover": return Image.PreserveAspectCrop
            case "fill": return Image.Stretch
            case "none": return Image.Pad
            default: return Image.PreserveAspectFit
        }
    }
    width: sized ? innerWidth : implicitWidth
    height: sized ? innerHeight : implicitHeight
    // A fitted image decodes to its box. An unsized image uses explicit Node
    // dimensions to bound decoding, or decodes at natural size.
    sourceSize.width: sized ? innerWidth : (host.fixedWidth > 0 ? host.fixedWidth : 0)
    sourceSize.height: sized ? innerHeight : (host.fixedHeight > 0 ? host.fixedHeight : 0)

    WheelHandler {
        // Only take the wheel when the plugin asked for it; otherwise a
        // surrounding scroll view keeps its native scrolling.
        enabled: picture.host.interactive && Props.bind(picture.host.model, "wheel") !== null
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onWheel: event => picture.host.invoke("wheel", event.angleDelta.y)
    }
}
