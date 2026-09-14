import QtQuick

// A line between things: a rule in a column, a divider in a row.
//
// The axis comes from the stack, not from this item's geometry. Measuring
// itself was circular — nought pixels thick along the way it lies means the
// width it read to decide was the width it had not been given, which drew
// every separator in a panel as a single pixel.
Rectangle {
    required property var host

    readonly property bool horizontal: host.axis !== "row"

    // A hairline, which on a scaled display is more than one device pixel.
    readonly property int thickness: host.space(1)

    // Nought along the way it lies: it asks for none of the room the layout is
    // about to hand it all of.
    implicitWidth: horizontal ? 0 : thickness
    implicitHeight: horizontal ? thickness : 0

    color: host.rule
}
