import QtQuick

// A line between things.
//
// Along whichever way the parent runs: a rule in a column is a divider in a
// row, and asking the author which they meant would be asking them to know
// what they already said by putting it in one.
Rectangle {
    required property var host

    readonly property bool horizontal: parent && parent.width > parent.height

    implicitWidth: horizontal ? 0 : 1
    implicitHeight: horizontal ? 1 : 0
    Layout.fillWidth: horizontal
    Layout.fillHeight: !horizontal
    width: horizontal ? (parent ? parent.width : 0) : 1
    height: horizontal ? 1 : (parent ? parent.height : 0)

    color: Qt.rgba(host.foreground.r, host.foreground.g, host.foreground.b, 0.2)
}
