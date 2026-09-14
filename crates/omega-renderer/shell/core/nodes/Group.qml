import QtQuick
import "../Props.js" as Props

// Mutually exclusive options. Selection is supplied by the model;
// activation submits the option's domain key.
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
            readonly property var selection: Props.selection_key(segment.modelData)
            readonly property string value: selection === null ? key : selection
            readonly property bool on: segment.key !== "" && segment.value === group.chosen
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
                if (segment.pressable) group.host.invoke("select", segment.value)
            }
            Keys.onReturnPressed: activate()
            Keys.onEnterPressed: activate()
            Keys.onSpacePressed: activate()

            Behavior on color {
                ColorAnimation { duration: host.theme.motion ? 120 : 0; easing.type: Easing.OutCubic }
            }

            // Load ViewNode by relative URL from the nodes directory.
            Loader {
                id: label
                anchors.centerIn: parent

                Component.onCompleted: setSource("../ViewNode.qml", {
                    "model": segment.modelData,
                    "session": Qt.binding(function() { return group.host.session }),
"theme": Qt.binding(function() { return group.host.theme }),
"assets": Qt.binding(function() { return group.host.assets }),
                    "foreground": Qt.binding(function() { return group.host.ink })
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
