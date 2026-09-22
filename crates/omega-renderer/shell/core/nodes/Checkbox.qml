import QtQuick
import "../Props.js" as Props
import "../Icons.js" as Icons

// Labelled checkbox with optimistic interaction feedback.
Item {
    id: box
    required property var host

    readonly property var bound: Props.bind(host.model, "change")
    readonly property bool published: Props.checkboxOn(host.model)

    // Optimistic state, reconciled when the published value or pending state
    // arrives. Assigned rather than bound: a ternary over mutable optimistic
    // state is reported as a binding loop on older Qt.
    property bool checked: false
    function sync() { box.checked = box.published }
    onPublishedChanged: box.sync()
    Connections {
        target: box.host
        function onPendingChanged() { if (!box.host.pending) box.sync() }
    }
    Component.onCompleted: box.sync()

    readonly property bool pressable: box.bound !== null && box.host.interactive
    enabled: box.bound !== null

    implicitWidth: row.implicitWidth
    implicitHeight: row.implicitHeight

    function activate() {
        if (!box.pressable) return
        var next = !box.checked
        box.checked = next
        if (!box.host.invoke("change", next)) box.checked = box.published
    }

    Row {
        id: row
        spacing: box.host.space(8)

        Rectangle {
            id: mark
            width: box.host.space(16)
            height: width
            radius: box.host.radius
            color: box.checked ? box.host.ink : box.host.trackFill
            border.width: box.host.space(1)
            border.color: box.host.rule
            activeFocusOnTab: true
            enabled: box.pressable
            Keys.onReturnPressed: box.activate()
            Keys.onEnterPressed: box.activate()
            Keys.onSpacePressed: box.activate()

            Text {
                anchors.centerIn: parent
                visible: box.checked
                text: Icons.glyph("check")
                color: box.host.theme.background
                font.family: box.host.fontFamily
                font.pixelSize: box.host.captionSize
            }
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: Props.checkboxLabel(box.host.model)
            visible: text !== ""
            color: box.host.ink
            font.family: box.host.fontFamily
            font.pixelSize: box.host.fontSize
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        enabled: box.pressable
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            mark.forceActiveFocus()
            box.activate()
        }
    }
}
