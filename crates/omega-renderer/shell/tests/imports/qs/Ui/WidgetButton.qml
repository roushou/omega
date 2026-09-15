import QtQuick

// Omarchy registers its outer buttons for click forwarding from the drag overlay.
Item {
    id: root
    property var bar: null
    property bool pressable: true
    property bool labelVisible: true
    property string tooltipText: ""
    property bool hasVisualContent: true
    property real fixedWidth: 0
    readonly property real scaledHorizontalMargin: 8
    readonly property color foreground: "white"
    implicitWidth: fixedWidth
    implicitHeight: 32
    signal pressed(int button)
    function triggerPress(button) { pressed(button) }
    Component.onCompleted: if (bar) bar.registerClickTarget(root)
    Component.onDestruction: if (bar) bar.unregisterClickTarget(root)
    MouseArea {
        anchors.fill: parent
        onClicked: mouse => { if (root.pressable) root.triggerPress(mouse.button) }
    }
}
