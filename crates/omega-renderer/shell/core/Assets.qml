import QtQuick

// Network access is never an implicit consequence of rendering a tree.
QtObject {
    property var iconResolver: null
    property var pendingImages: []
    property var failedImages: []
    function imageState(image, status) {
        var pending = pendingImages.filter(item => item !== image)
        var failed = failedImages.filter(item => item !== image)
        if (status === Image.Loading) pending.push(image)
        if (status === Image.Error) failed.push(image)
        pendingImages = pending
        failedImages = failed
    }
    function resolve(source) {
        if (typeof source !== "string") return ""
        if (source.startsWith("icon://")) return iconResolver ? iconResolver(source.slice(7)) : ""
        if (source.startsWith("/") || source.startsWith("file://") || source.startsWith("data:image/")) return source
        return ""
    }
}
