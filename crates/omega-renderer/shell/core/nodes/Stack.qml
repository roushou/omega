import QtQuick
import QtQml.Models
import QtQuick.Layouts
import "../Props.js" as Props

// Row or column layout with keyed delegates. Load children by URL to support recursion.
Loader {
    id: stack
    required property var host

    readonly property bool column:
        Props.stackAlign(host.model) === "column"

    sourceComponent: stack.column ? columnLayout : rowLayout

    // Retain matching delegates while updating their QVariantMap payloads.
    ListModel { id: rows; dynamicRoles: true }

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

    // Remove what is gone, then walk the wanted order moving or inserting.
    // Quadratic, which for a stack of this size costs less than the layout.
    function reconcile() {
        var wanted = stack.wanted

        function keyOf(node) {
            return node && node.key ? node.key : ""
        }

        // Anything whose key is no longer wanted.
        for (var i = rows.count - 1; i >= 0; i--) {
            var held = rows.get(i).key
            var stillWanted = false
            for (var w = 0; w < wanted.length; w++) {
                if (keyOf(wanted[w]) === held) { stillWanted = true; break }
            }
            if (!stillWanted) rows.remove(i)
        }

        for (var target = 0; target < wanted.length; target++) {
            var key = keyOf(wanted[target])

            var found = -1
            for (var j = target; j < rows.count; j++) {
                if (rows.get(j).key === key) { found = j; break }
            }

            if (found === -1) {
                rows.insert(target, { "key": key, "node": wanted[target] })
                continue
            }
            if (found !== target) rows.move(found, target, 1)
            // The node object changes every render even when its key does not,
            // so the delegate is kept and its data replaced.
            rows.setProperty(target, "node", wanted[target])
        }

        while (rows.count > wanted.length) rows.remove(rows.count - 1)
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
"theme": Qt.binding(function() { return stack.host.theme }),
"assets": Qt.binding(function() { return stack.host.assets }),
                "foreground": Qt.binding(function() { return stack.host.ink }),
                "axis": stack.column ? "column" : "row"
            })

            onNodeChanged: if (child.item) child.item.model = child.node
        }
    }
}
