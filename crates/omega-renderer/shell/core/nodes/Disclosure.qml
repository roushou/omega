import QtQuick
import "../Props.js" as Props

// A heading that reveals or hides its children inline.
Item {
    id: disclosure
    required property var host

    readonly property var bound: Props.bind(host.model, "toggle")
    readonly property bool published: Props.disclosureOpen(host.model)

    // Optimistic state, reconciled when the published value or pending state
    // arrives. Assigned rather than bound to avoid a binding loop on older Qt.
    property bool open: false
    function sync() { disclosure.open = disclosure.published }
    onPublishedChanged: disclosure.sync()
    Connections {
        target: disclosure.host
        function onPendingChanged() { if (!disclosure.host.pending) disclosure.sync() }
    }
    Component.onCompleted: disclosure.sync()

    readonly property bool pressable: disclosure.bound !== null && disclosure.host.interactive

    function toggle() {
        if (!disclosure.pressable) return
        var next = !disclosure.open
        disclosure.open = next
        if (!disclosure.host.invoke("toggle", next)) disclosure.open = disclosure.published
    }

    implicitWidth: Math.max(header.implicitWidth, body.implicitWidth)
    implicitHeight: header.implicitHeight
        + (disclosure.open ? body.implicitHeight + disclosure.host.space(6) : 0)

    Rectangle {
        id: header
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Math.max(disclosure.host.space(32), title.implicitHeight + disclosure.host.space(8))
        radius: disclosure.host.radius
        color: headerHover.containsMouse ? disclosure.host.hoverFill : "transparent"
        activeFocusOnTab: true
        enabled: disclosure.pressable
        border.width: header.activeFocus ? disclosure.host.space(1) : 0
        border.color: disclosure.host.ink
        Keys.onReturnPressed: disclosure.toggle()
        Keys.onEnterPressed: disclosure.toggle()
        Keys.onSpacePressed: disclosure.toggle()

        Text {
            id: title
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.leftMargin: disclosure.host.space(8)
            text: Props.disclosureTitle(disclosure.host.model)
            color: disclosure.host.ink
            font.family: disclosure.host.fontFamily
            font.pixelSize: disclosure.host.fontSize
            font.bold: true
        }
    }

    Column {
        id: body
        anchors.top: header.bottom
        anchors.topMargin: disclosure.host.space(6)
        anchors.left: parent.left
        anchors.right: parent.right
        visible: disclosure.open

        Repeater {
            model: disclosure.open ? Props.children(disclosure.host.model) : []
            delegate: child
        }
    }

    Component {
        id: child
        Loader {
            id: cell
            required property var modelData
            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": cell.modelData,
                "session": Qt.binding(function() { return disclosure.host.session }),
                "theme": Qt.binding(function() { return disclosure.host.theme }),
                "assets": Qt.binding(function() { return disclosure.host.assets }),
                "foreground": Qt.binding(function() { return disclosure.host.ink })
            })
        }
    }

    MouseArea {
        id: headerHover
        anchors.fill: header
        hoverEnabled: true
        enabled: disclosure.pressable
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            header.forceActiveFocus()
            disclosure.toggle()
        }
    }
}
