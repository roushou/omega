import QtQuick
import "../Props.js" as Props

// One of a few, chosen.
//
// Joined, so it reads as one control with several settings rather than
// several controls. Which one is on comes from the unit; pressing another
// reports its key and the unit decides what that means — an optimistic flip
// here would be a control claiming an authority it does not have, because
// unlike a toggle there is no obvious next state.
Row {
    id: group
    required property var host

    readonly property var bound: Props.bind(host.model, "select")
    readonly property string chosen: Props.groupSelected(host.model)

    spacing: host.space(1)
    readonly property var options: Props.children(group.host.model)
    property var keys: []
    function sync() {
        var next = []
        for (var i = 0; i < group.options.length; i++) next.push(group.options[i].key || "")
        if (JSON.stringify(next) !== JSON.stringify(group.keys)) group.keys = next
    }
    onOptionsChanged: sync()
    Component.onCompleted: sync()

    Repeater {
        model: group.keys

        delegate: Rectangle {
            id: segment
            required property int index
            readonly property var modelData: group.options[segment.index]

            readonly property string key: modelData && modelData.key ? modelData.key : ""
            readonly property bool on: segment.key !== "" && segment.key === group.chosen
            readonly property bool hot:
                hover.containsMouse && group.bound !== null
                && group.host.interactive && segment.key !== ""

            implicitWidth: label.implicitWidth + group.host.space(16)
            implicitHeight: label.implicitHeight + group.host.space(8)
            radius: group.host.radius
            // Chosen, being pointed at, or neither — the three states the
            // rest of the shell's controls draw, at the theme's own alphas.
            color: segment.on
                ? group.host.chosenFill
                : (segment.hot ? group.host.hoverFill : group.host.idleFill)

            readonly property bool pressable: segment.enabled && group.host.interactive
            activeFocusOnTab: true
            enabled: group.bound !== null && segment.key !== "" && !Props.disabled(segment.modelData) && !Props.busy(segment.modelData)
            border.width: segment.activeFocus ? group.host.space(1) : 0
            border.color: group.host.ink
            function activate() {
                if (segment.pressable) group.host.invoke(group.bound, segment.key)
            }
            Keys.onReturnPressed: activate()
            Keys.onEnterPressed: activate()
            Keys.onSpacePressed: activate()

            Behavior on color {
                ColorAnimation { duration: 120; easing.type: Easing.OutCubic }
            }

            // By url, like a stack's children and a grid's cells: from
            // `nodes/` the name `ViewNode` resolves to nothing, and naming
            // it as a type left every option blank.
            Loader {
                id: label
                anchors.centerIn: parent

                Component.onCompleted: setSource("../ViewNode.qml", {
                    "model": segment.modelData,
                    "connection": group.host.connection,
                    "foreground": group.host.ink
                })
            }

            Connections {
                target: segment
                function onModelDataChanged() { if (label.item) label.item.model = segment.modelData }
            }

            MouseArea {
                id: hover
                anchors.fill: parent
                hoverEnabled: true
                enabled: segment.pressable
                cursorShape: Qt.PointingHandCursor
                onClicked: { segment.forceActiveFocus(); segment.activate() }
            }
        }
    }
}
