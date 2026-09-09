import QtQuick
import QtQml.Models
import "../Props.js" as Props

// Rows to choose from.
//
// A stack draws children in a line; this is the one the user moves through.
// Arrows change the selection and Enter activates it, and neither reaches the
// unit: where the cursor is right now is what the user is doing, not something
// the machine knows. Only the activation is reported, carrying the row's key.
//
// Rows are held by key, like a stack's children, so a list that refreshes
// twice a second does not rebuild the row somebody is reading.
Rectangle {
    id: list
    required property var host

    readonly property var bound: Props.bind(host.model, "activate")
    readonly property var wanted: Props.children(host.model)
    readonly property int gap: Props.listGap(host.model)
    readonly property int fixedHeight: Props.height(host.model)

    // Which row the cursor is on. -1 is none, which is where a list starts:
    // showing a selection nobody made would have the first Enter do something
    // the user did not choose.
    property int selected: -1

    color: "transparent"
    implicitWidth: rows.implicitWidth
    // As tall as its rows, up to a point: a list of forty networks in a
    // popout that grew to fit them would be a popout taller than the screen.
    // A unit that knows better says so with `height`.
    readonly property int cap: 400
    implicitHeight: list.fixedHeight > 0
        ? list.fixedHeight
        : Math.min(rows.contentHeight, list.cap)

    onWantedChanged: list.reconcile()
    Component.onCompleted: list.reconcile()

    function keyOf(node) {
        return node && node.key ? node.key : ""
    }

    // The same keyed reconcile a stack does: remove what is gone, then walk
    // the wanted order moving or inserting, replacing each row's data so a
    // kept delegate is handed the new reading through its bindings.
    function reconcile() {
        var wanted = list.wanted

        for (var i = held.count - 1; i >= 0; i--) {
            // Not `held`: `var` is function-scoped and hoisted, so a local of
            // that name would shadow the model for the whole function and
            // every `held.count` would read undefined.
            var existing = held.get(i).key
            var stillWanted = false
            for (var w = 0; w < wanted.length; w++) {
                if (list.keyOf(wanted[w]) === existing) { stillWanted = true; break }
            }
            if (!stillWanted) held.remove(i)
        }

        for (var target = 0; target < wanted.length; target++) {
            var key = list.keyOf(wanted[target])
            var found = -1
            for (var j = target; j < held.count; j++) {
                if (held.get(j).key === key) { found = j; break }
            }
            if (found === -1) {
                held.insert(target, { "key": key, "node": wanted[target] })
                continue
            }
            if (found !== target) held.move(found, target, 1)
            held.setProperty(target, "node", wanted[target])
        }

        while (held.count > wanted.length) held.remove(held.count - 1)

        if (list.selected >= held.count) list.selected = held.count - 1
    }

    function activate(index) {
        if (list.bound === null || index < 0 || index >= held.count) return
        list.host.invoke(list.bound, held.get(index).key)
    }

    ListModel { id: held }

    ListView {
        id: rows
        anchors.fill: parent
        model: held
        spacing: list.gap
        clip: true
        // Only take keys when there is something to do with them; a list
        // nothing is bound to should not swallow the panel's navigation.
        focus: list.bound !== null && list.host.interactive

        Keys.onUpPressed: list.selected = Math.max(0, list.selected - 1)
        Keys.onDownPressed: list.selected = Math.min(held.count - 1, list.selected + 1)
        Keys.onReturnPressed: list.activate(list.selected)
        Keys.onEnterPressed: list.activate(list.selected)

        // Follow the cursor rather than let it walk off the visible rows.
        onCurrentIndexChanged: rows.positionViewAtIndex(rows.currentIndex, ListView.Contain)
        currentIndex: list.selected

        delegate: Item {
            id: row
            required property int index
            required property var node

            width: rows.width
            implicitHeight: child.item ? child.item.implicitHeight : 0

            Rectangle {
                anchors.fill: parent
                radius: 3
                visible: row.index === list.selected
                color: Qt.rgba(list.host.foreground.r, list.host.foreground.g,
                               list.host.foreground.b, 0.12)
            }

            Loader {
                id: child
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter

                Component.onCompleted: setSource("../ViewNode.qml", {
                    "model": row.node,
                    "foreground": list.host.foreground
                })

                Connections {
                    target: child.item
                    function onInvoke(bound, value) { list.host.invoke(bound, value) }
                }
            }

            Connections {
                target: row
                function onNodeChanged() { if (child.item) child.item.model = row.node }
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton
                z: -1
                onClicked: {
                    list.selected = row.index
                    list.activate(row.index)
                }
            }
        }
    }
}
