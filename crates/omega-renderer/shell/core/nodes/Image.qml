import QtQuick
import "../Props.js" as Props

// Local-file and data-URI images. Load asynchronously to avoid blocking rendering.
Image {
    id: picture
    required property var host
    onStatusChanged: host.assets.imageState(picture, status)
    Component.onDestruction: host.assets.imageState(picture, Image.Null)

    source: host.assets.resolve(Props.imageSource(host.model))
    visible: source !== "" && status === Image.Ready
    asynchronous: true
    cache: true
    fillMode: Image.PreserveAspectFit
    // Use the dimensions already applied by ViewNode.
    sourceSize.width: host.fixedWidth > 0 ? host.fixedWidth : 0
    sourceSize.height: host.fixedHeight > 0 ? host.fixedHeight : 0
}
