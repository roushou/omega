import QtQuick

// Draw a separator perpendicular to the parent stack axis.
// Derive orientation from the parent, not this item's size, to avoid circular sizing.
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
