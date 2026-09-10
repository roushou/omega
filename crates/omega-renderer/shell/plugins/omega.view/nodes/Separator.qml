import QtQuick

// A line between things.
//
// Along whichever way the parent runs: a rule in a column is a divider in a
// row, and asking the author which they meant would be asking them to know
// what they already said by putting it in one.
//
// Which way that is comes from the stack, not from this item's own geometry.
// Measuring itself was circular — a rule is nought pixels thick along the way
// it lies, so the width it was reading to decide was the width it had not
// been given yet, and every separator in a panel drew as a single pixel.
Rectangle {
    required property var host

    readonly property bool horizontal: host.axis !== "row"

    // A hairline, which on a scaled display is more than one device pixel.
    readonly property int thickness: host.space(1)

    // Nought along the way it lies, so it asks the stack for none of the room
    // it is about to be handed all of. `ViewNode` gives it that length; the
    // layout decides how much there is.
    implicitWidth: horizontal ? 0 : thickness
    implicitHeight: horizontal ? thickness : 0

    color: host.rule
}
