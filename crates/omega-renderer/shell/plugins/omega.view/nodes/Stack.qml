import QtQuick
import QtQml.Models
import QtQuick.Layouts
import "../Props.js" as Props

// Children in a line, along one axis or the other.
//
// A layout, not a positioner: positioners leave children at their own size and
// top-align them, and a child whose width came from the width it helped decide
// is a binding loop. Children load by url — QML refuses a component that names
// itself — and are keyed, so a delegate mid-interaction survives its
// neighbours changing.
Loader {
    id: stack
    required property var host

    readonly property bool column:
        Props.stackAlign(host.model) === "column"

    sourceComponent: stack.column ? columnLayout : rowLayout

    // Mirrors this stack's children, updated in place so a delegate survives
    // its neighbours changing. Rebuilt wholesale only when nothing matches.
    ListModel { id: rows }

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

            // The slack, at the trailing edge. Stands down once a child claims
            // the room. A column needs none: the surface holding one is as tall
            // as what it draws.
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

    // `model` is bound rather than passed at construction, so a delegate kept
    // across a reorder is handed its new node through the same binding.
    Component {
        id: childNode
        Loader {
            id: child
            required property var node

            // A rule lies across its stack. A stack inside a column spans it
            // unasked, because a `fill` chain down every level of a panel is a
            // chain nobody writes. Along the axis a stack runs, room goes only
            // to a node that asked; a node with a width of its own has said
            // what it wants instead of either.
            readonly property bool rule: child.node && child.node.type === "separator"
            readonly property bool block: stack.column && child.node && child.node.type === "stack"
            readonly property bool spans:
                child.rule
                    ? stack.column
                    : ((child.block || Props.fill(child.node))
                        && Props.width(child.node) === 0)

            Layout.fillWidth: child.spans
            Layout.fillHeight: child.rule && !stack.column
            // Centred across the axis, at the size it draws. A spanning child
            // names no alignment in the direction it spans, or that alignment
            // would pin it to its preferred width.
            Layout.alignment: stack.column
                ? (child.spans ? Qt.AlignVCenter : (Qt.AlignLeft | Qt.AlignVCenter))
                : (child.rule ? Qt.AlignHCenter : Qt.AlignVCenter)

            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": child.node,
                "foreground": stack.host.foreground,
                "axis": stack.column ? "column" : "row"
            })

            onNodeChanged: if (child.item) child.item.model = child.node

            Connections {
                target: child.item
                function onInvoke(bound, value) { stack.host.invoke(bound, value) }
            }
        }
    }
}
