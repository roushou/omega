import QtQuick
import "../Props.js" as Props

// A picture.
//
// The source is a local file or a `data:` URI; the SDK drops anything else
// before it reaches the wire, so an empty source here is a unit that asked
// for something this shell will not fetch. `asynchronous` because a large
// file read on the render thread stalls the whole bar, not just this node.
Image {
    required property var host

    source: Props.text(host.model, "source", "")
    visible: source !== "" && status === Image.Ready
    asynchronous: true
    cache: true
    fillMode: Image.PreserveAspectFit
    // The size a node asked for, which `ViewNode` has already applied to the
    // slot. Without this the picture is drawn at its own pixel size and
    // ignores it.
    sourceSize.width: host.fixedWidth > 0 ? host.fixedWidth : 0
    sourceSize.height: host.fixedHeight > 0 ? host.fixedHeight : 0
}
