import QtQuick

Item {
    property var anchorItem
    property var owner
    property var bar
    property bool open: false
    visible: open
    property var focusTarget
    property real contentWidth: 0
    property real contentHeight: 0
    property real padding: 0
    function fittedContentWidth(value, maximum) { return Math.min(value, maximum) }
    function fittedContentHeight(value) { return value }
}
