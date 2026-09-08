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

    columns: Math.max(1, Props.number(host.model, "columns", 1))
    spacing: Props.number(host.model, "gap", 0)

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
                "foreground": grid.host.foreground
            })

            Connections {
                target: child.item
                function onInvoke(bound, value) { grid.host.invoke(bound, value) }
            }
        }
    }
}
