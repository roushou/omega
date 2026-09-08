import QtQuick
import QtQml.Models
import "../Props.js" as Props

// Children in a line, along one axis or the other.
//
// Each child is a `ViewNode` again, loaded by url so the recursion resolves at
// runtime. Presses arriving from below are forwarded up, so the host at the
// top sees every one of them.
//
// Children are held by **key**, not by position. A `Repeater` over a plain
// array rebuilds every delegate whenever the array changes, which is fine for
// text and wrong for anything holding state: a row of sliders that reorders
// would lose the drag in progress, and a list that refreshes twice a second
// would lose it continuously. The keys are the ones the SDK assigns — the
// path to a node, or the identity an author gave it with `.key()`.
Loader {
    id: stack
    required property var host

    readonly property bool column:
        Props.text(host.model, "align", "row") === "column"

    sourceComponent: stack.column ? columnLayout : rowLayout

    // Mirrors this stack's children, updated in place so a delegate survives
    // its neighbours changing. Rebuilt wholesale only when nothing matches.
    ListModel { id: rows }

    readonly property var wanted: Props.children(stack.host.model)
    onWantedChanged: stack.reconcile()
    Component.onCompleted: stack.reconcile()

    // Bring `rows` to the incoming children, keeping every node whose key is
    // still there.
    //
    // Deliberately simple: remove what is gone, then walk the wanted order
    // moving or inserting as it goes. It is quadratic in the number of
    // children, which for a bar widget is a handful and for a panel list is
    // still nothing next to laying them out.
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
            // The node object itself changes every render even when the key
            // does not — a new reading, a new label — so the delegate is kept
            // and its data replaced.
            rows.setProperty(target, "node", wanted[target])
        }

        while (rows.count > wanted.length) rows.remove(rows.count - 1)
    }

    Component {
        id: rowLayout
        Row {
            spacing: Props.number(stack.host.model, "gap", 0)
            Repeater {
                model: rows
                delegate: childNode
            }
        }
    }

    Component {
        id: columnLayout
        Column {
            spacing: Props.number(stack.host.model, "gap", 0)
            Repeater {
                model: rows
                delegate: childNode
            }
        }
    }

    // One child, loaded by url so the recursion resolves at runtime.
    //
    // `model` is bound rather than passed at construction: a delegate kept
    // across a reorder is handed a new node through the same binding, which
    // is the whole point of keying them.
    Component {
        id: childNode
        Loader {
            id: child
            required property var node

            Component.onCompleted: setSource("../ViewNode.qml", {
                "model": child.node,
                "foreground": stack.host.foreground
            })

            onNodeChanged: if (child.item) child.item.model = child.node

            Connections {
                target: child.item
                function onInvoke(bound, value) { stack.host.invoke(bound, value) }
            }
        }
    }
}
