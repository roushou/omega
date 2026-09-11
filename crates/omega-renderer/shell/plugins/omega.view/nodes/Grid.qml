import QtQuick
import "../Props.js" as Props

// Children in rows of a fixed width.
//
// A `Grid` rather than a stack of rows because the columns have to line up:
// a label beside a figure, four times over, wants one width for the labels
// and not four.
Grid {
    id: grid
    required property var host

    columns: Math.max(1, Props.gridColumns(host.model))
    spacing: host.space(Props.gridGap(host.model))

    Repeater {
        model: Props.children(grid.host.model)
        delegate: cell
    }

    // Loaded by url, like a stack's children: declaring `modelData` on a
    // `ViewNode` would make it a property that type requires, which is not
    // what a repeater injecting one means.
    Component {
        id: cell
        Loader {
            id: child
            required property var modelData

            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": child.modelData,
                "connection": grid.host.connection,
                "foreground": grid.host.ink
            })
        }
    }
}
