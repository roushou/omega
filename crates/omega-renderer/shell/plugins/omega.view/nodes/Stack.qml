import QtQuick
import QtQml.Models
import QtQuick.Layouts
import "../Props.js" as Props

// Children in a line, along one axis or the other.
//
// Each child is a `ViewNode` again, loaded by url so the recursion resolves at
// runtime. Presses arriving from below are forwarded up, so the host at the
// top sees every one of them.
//
// A **layout**, not a positioner. `Row` and `Column` place children and
// nothing else: they leave every child at its own size and hang them all from
// a common top, which is why a header of an icon, a display figure and a
// caption used to read as three things starting at once rather than one line.
// A layout centres them across the way it runs, and — the part a positioner
// cannot do at all — hands a child that asked for the room the room, without
// the loop that binding a child's width to the width its own width decides
// would be.
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
        Props.stackAlign(host.model) === "column"

    sourceComponent: stack.column ? columnLayout : rowLayout

    // Mirrors this stack's children, updated in place so a delegate survives
    // its neighbours changing. Rebuilt wholesale only when nothing matches.
    ListModel { id: rows }

    readonly property var wanted: Props.children(stack.host.model)

    // Whether any child has asked for the room this stack has to give.
    //
    // A layout with room to spare and nobody claiming it spreads the room
    // between its cells, which is not what a row of an icon and the figure
    // beside it means by being wide. So when nothing claims it, the trailing
    // item below does — and the children pack from the leading edge, the way
    // they did when a row was only ever as wide as its contents.
    readonly property bool claimed: {
        var wanted = stack.wanted
        for (var i = 0; i < wanted.length; i++) {
            if (Props.fill(wanted[i]) && Props.width(wanted[i]) === 0) return true
        }
        return false
    }
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
        RowLayout {
            spacing: stack.host.space(Props.stackGap(stack.host.model))
            Repeater {
                model: rows
                delegate: childNode
            }

            // The slack, at the end. Nothing to draw and no room of its own:
            // it is here to be the one cell that grows, and it stands down
            // the moment a child of the stack asks for the room itself — a
            // spacer between two things, or a bar told to span.
            //
            // A column needs no such thing: the surface holding one is as
            // tall as what it draws, so there is no room going spare down
            // the way a column runs.
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

            // What a child does with the room the stack has.
            //
            // A rule is the one node whose shape is its parent's business: it
            // lies across the way the stack runs, so in a column it takes the
            // width and in a row the height.
            //
            // A stack inside a column takes the width too, without being
            // asked. A block inside a block is as wide as the block — and
            // the alternative was every panel writing `fill` down each level
            // of itself before a bar at the bottom could span anything,
            // which is a chain nobody remembers and which fails by drawing a
            // gauge the width of the sentence beneath it. It costs nothing
            // to look at: the slack goes to the end of a row rather than
            // between its children, so a row that spans draws what a row
            // that hugs drew until something inside asks for the room.
            //
            // Along the way a stack *runs*, room goes only to a node that
            // asked for it. That is the direction where handing it out would
            // move everything else — a row's slack is what a spacer is for.
            // And a node that named a width of its own has already said what
            // it wants instead of either.
            readonly property bool rule: child.node && child.node.type === "separator"
            readonly property bool block: stack.column && child.node && child.node.type === "stack"
            readonly property bool spans:
                child.rule
                    ? stack.column
                    : ((child.block || Props.fill(child.node))
                        && Props.width(child.node) === 0)

            Layout.fillWidth: child.spans
            Layout.fillHeight: child.rule && !stack.column
            // Across the way the stack runs, a child is centred at the size
            // it draws — which is what makes a caption beside a display
            // figure sit on the line rather than above it. A child that
            // spans is given the cell instead, so no alignment is named for
            // the direction it spans in.
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
