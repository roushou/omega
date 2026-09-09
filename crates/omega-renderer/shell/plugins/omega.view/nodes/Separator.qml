import QtQuick

// A line between things.
//
// Along whichever way the parent runs: a rule in a column is a divider in a
// row, and asking the author which they meant would be asking them to know
// what they already said by putting it in one.
Rectangle {
    required property var host

    readonly property bool horizontal: parent && parent.width > parent.height

    // A hairline, which on a scaled display is more than one device pixel.
    readonly property int thickness: host.space(1)

    implicitWidth: horizontal ? 0 : thickness
    implicitHeight: horizontal ? thickness : 0
    // No `Layout.fillWidth` here: a stack is a `Row` or a `Column`, not a
    // layout, so the attached object does not exist and the binding was a
    // warning on every separator drawn.
    width: horizontal ? (parent ? parent.width : 0) : thickness
    height: horizontal ? thickness : (parent ? parent.height : 0)

    color: host.rule
}
