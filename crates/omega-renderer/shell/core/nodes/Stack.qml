import QtQuick
import QtQuick.Layouts
import ".."
import "../Props.js" as Props

// Row or column layout with keyed delegates. Load children by URL to support recursion.
Loader {
    id: stack
    required property var host

    readonly property bool column:
        Props.stackAlign(host.model) === "column"

    sourceComponent: stack.column ? columnLayout : rowLayout

    // Retain matching delegates while updating their QVariantMap payloads.
    KeyedChildren { id: rows }

    readonly property var wanted: Props.children(stack.host.model)

    // A layout with room to spare and no claimant spreads it between the cells,
    // so an unclaimed row gives the slack to the trailing item instead.
    readonly property bool claimed: {
        var wanted = stack.wanted
        for (var i = 0; i < wanted.length; i++) {
            if (Props.fill(wanted[i]) && Props.width(wanted[i]) === 0) return true
        }
        return false
    }
    onWantedChanged: stack.reconcile()
    Component.onCompleted: stack.reconcile()

    function reconcile() {
        rows.reconcile(stack.wanted)
        // Refresh cached layout items before resize can access removed delegates (Qt 6.4).
        if (stack.item) stack.item.ensurePolished()
    }

    Component {
        id: rowLayout
        RowLayout {
            spacing: stack.host.space(Props.stackGap(stack.host.model))
            Repeater {
                model: rows
                delegate: childNode
            }

            // Trailing row space applies only when no child requests expansion.
            Item {
                Layout.fillWidth: !stack.claimed
                implicitWidth: 0
                implicitHeight: 0
            }
        }
    }

    Component {
        id: columnLayout
        ColumnLayout {
            spacing: stack.host.space(Props.stackGap(stack.host.model))
            Repeater {
                model: rows
                delegate: childNode
            }
        }
    }

    // Bind delegates to current model data so reused items receive reordered payloads.
    Component {
        id: childNode
        Loader {
            id: child
            required property var node

            // Block content spans a column unless it declares a fixed width.
            readonly property bool rule: child.node && child.node.type === "separator"
            readonly property bool block: stack.column && child.node && ["stack", "form", "field", "slider", "progress", "graph", "list", "text", "header"].indexOf(child.node.type) !== -1
            readonly property bool spans:
                child.rule
                    ? stack.column
                    : ((child.block || Props.fill(child.node))
                        && Props.width(child.node) === 0)

            Layout.minimumWidth: 0
            Layout.fillWidth: child.spans
            Layout.fillHeight: child.rule && !stack.column
            // Center fixed-size children across the axis; expanding children have no alignment constraint.
            Layout.alignment: stack.column
                ? (child.spans ? Qt.AlignVCenter : (Qt.AlignLeft | Qt.AlignVCenter))
                : (child.rule ? Qt.AlignHCenter : Qt.AlignVCenter)

            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": child.node,
                "session": Qt.binding(function() { return stack.host.session }),
                "navigation": Qt.binding(function() { return stack.host.navigation || null }),
                "theme": Qt.binding(function() { return stack.host.theme }),
                "assets": Qt.binding(function() { return stack.host.assets }),
                "foreground": Qt.binding(function() { return stack.host.ink }),
                "axis": stack.column ? "column" : "row"
            })

            onNodeChanged: if (child.item) child.item.model = child.node
        }
    }
}
