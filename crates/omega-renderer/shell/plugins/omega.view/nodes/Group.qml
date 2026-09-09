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

    spacing: 1

    Repeater {
        model: Props.children(group.host.model)

        delegate: Rectangle {
            id: segment
            required property var modelData

            readonly property string key: modelData && modelData.key ? modelData.key : ""
            readonly property bool on: segment.key !== "" && segment.key === group.chosen

            implicitWidth: label.implicitWidth + 16
            implicitHeight: label.implicitHeight + 8
            color: segment.on
                ? Qt.rgba(group.host.ink.r, group.host.ink.g, group.host.ink.b, 0.25)
                : Qt.rgba(group.host.foreground.r, group.host.foreground.g,
                          group.host.foreground.b, 0.08)

            ViewNode {
                id: label
                anchors.centerIn: parent
                model: segment.modelData
                foreground: group.host.foreground
                onInvoke: (bound, value) => group.host.invoke(bound, value)
            }

            MouseArea {
                anchors.fill: parent
                enabled: group.bound !== null && group.host.interactive && segment.key !== ""
                cursorShape: Qt.PointingHandCursor
                onClicked: group.host.invoke(group.bound, segment.key)
            }
        }
    }
}
