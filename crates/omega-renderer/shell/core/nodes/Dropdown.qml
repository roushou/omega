import QtQuick
import "../Props.js" as Props

// A collapsed select that expands inline to its keyed options. The renderer
// owns the popover as an inline panel; the SDK declares options, selection,
// and the typed selection binding.
Item {
    id: dropdown
    required property var host

    readonly property var bound: Props.bind(host.model, "select")
    readonly property string chosen: Props.dropdownSelected(host.model)
    readonly property var options: Props.children(host.model)
    property bool open: false
    readonly property bool pressable: dropdown.bound !== null && dropdown.host.interactive

    implicitWidth: Math.max(host.space(160), selector.implicitWidth)
    implicitHeight: selector.implicitHeight
        + (dropdown.open ? menu.implicitHeight + dropdown.host.space(4) : 0)

    function valueOf(node) {
        var value = Props.selection_key(node)
        return value === null ? (node && node.key ? node.key : "") : value
    }
    function selectedNode() {
        if (dropdown.chosen === "" || dropdown.chosen === null) return null
        for (var i = 0; i < dropdown.options.length; i++) {
            if (dropdown.valueOf(dropdown.options[i]) === dropdown.chosen) return dropdown.options[i]
        }
        return null
    }

    // The collapsed trigger.
    Rectangle {
        id: selector
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Math.max(dropdown.host.space(36), triggerRow.implicitHeight + dropdown.host.space(16))
        radius: dropdown.host.radius
        color: trigger.containsMouse ? dropdown.host.hoverFill : dropdown.host.idleFill
        activeFocusOnTab: true
        enabled: dropdown.pressable
        border.width: selector.activeFocus ? dropdown.host.space(1) : 0
        border.color: dropdown.host.ink

        function toggle() {
            if (!dropdown.pressable) return
            dropdown.open = !dropdown.open
        }
        Keys.onReturnPressed: selector.toggle()
        Keys.onEnterPressed: selector.toggle()
        Keys.onSpacePressed: selector.toggle()

        Row {
            id: triggerRow
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.leftMargin: dropdown.host.space(10)
            anchors.right: parent.right
            anchors.rightMargin: dropdown.host.space(10)
            spacing: dropdown.host.space(8)

            Loader {
                id: selected
                anchors.verticalCenter: parent.verticalCenter
                readonly property var node: dropdown.selectedNode()
                Component.onCompleted: selected.load()
                onNodeChanged: selected.load()
                function load() {
                    if (selected.node === null) { setSource(""); return }
                    setSource("../ViewNode.qml", {
                        "model": selected.node,
                        "session": Qt.binding(function() { return dropdown.host.session }),
                        "theme": Qt.binding(function() { return dropdown.host.theme }),
                        "assets": Qt.binding(function() { return dropdown.host.assets }),
                        "foreground": Qt.binding(function() { return dropdown.host.ink })
                    })
                }
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: selected.node === null
                text: Props.dropdownPlaceholder(dropdown.host.model)
                color: Qt.darker(dropdown.host.ink, 1.4)
                font.family: dropdown.host.fontFamily
                font.pixelSize: dropdown.host.fontSize
            }
        }
    }

    MouseArea {
        id: trigger
        anchors.fill: selector
        hoverEnabled: true
        enabled: dropdown.pressable
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            selector.forceActiveFocus()
            selector.toggle()
        }
    }

    // The inline option list, shown only while open.
    Column {
        id: menu
        anchors.top: selector.bottom
        anchors.topMargin: dropdown.host.space(4)
        anchors.left: parent.left
        anchors.right: parent.right
        visible: dropdown.open
        spacing: dropdown.host.space(2)

        Repeater {
            model: dropdown.open ? dropdown.options : []
            delegate: option
        }
    }

    Component {
        id: option
        Rectangle {
            id: optionRow
            required property var modelData
            readonly property string key: dropdown.valueOf(optionRow.modelData)
            readonly property bool on: optionRow.key !== "" && optionRow.key === dropdown.chosen
            readonly property bool hot:
                optionHover.containsMouse && dropdown.pressable && optionRow.key !== ""

            height: Math.max(dropdown.host.space(32), label.implicitHeight + dropdown.host.space(8))
            radius: dropdown.host.radius
            color: optionRow.on
                ? dropdown.host.chosenFill
                : (optionRow.hot ? dropdown.host.hoverFill : "transparent")

            readonly property bool pressable: dropdown.pressable
                && optionRow.key !== ""
                && !Props.disabled(optionRow.modelData)
                && !Props.busy(optionRow.modelData)
            enabled: optionRow.pressable

            function activate() {
                if (optionRow.pressable) dropdown.host.invoke("select", optionRow.key)
            }

            Loader {
                id: label
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: dropdown.host.space(8)
                anchors.right: parent.right
                anchors.rightMargin: dropdown.host.space(8)
                Component.onCompleted: setSource("../ViewNode.qml", {
                    "model": optionRow.modelData,
                    "session": Qt.binding(function() { return dropdown.host.session }),
                    "theme": Qt.binding(function() { return dropdown.host.theme }),
                    "assets": Qt.binding(function() { return dropdown.host.assets }),
                    "foreground": Qt.binding(function() { return dropdown.host.ink })
                })
            }

            MouseArea {
                id: optionHover
                anchors.fill: parent
                hoverEnabled: true
                enabled: optionRow.pressable
                cursorShape: Qt.PointingHandCursor
                onClicked: optionRow.activate()
            }
        }
    }
}
