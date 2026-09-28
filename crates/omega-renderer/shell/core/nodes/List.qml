import QtQuick
import ".."
import "../Props.js" as Props

// Keyed delegates preserve rows across updates. Navigation is immediate in QML;
// optional selection bindings report semantic values to the instance, and
// controlled selection follows the published model. Activation is a separate event.
Rectangle {
    id: list
    required property var host

    readonly property var bound: Props.bind(host.model, "activate")
    readonly property var wanted: Props.children(host.model)
    readonly property int gap: Props.listGap(host.model)
    readonly property int fixedHeight: Props.height(host.model)

    // Keyboard cursor index. `-1` means no row is selected.
    property int selected: -1

    color: "transparent"
    border.width: rows.activeFocus ? list.host.space(1) : 0
    border.color: list.host.ink
    radius: list.host.radius
    implicitWidth: host.space(280)
    // Cap list height unless the node specifies an explicit height.
    readonly property int cap: host.space(400)
    implicitHeight: list.fixedHeight > 0
        ? list.fixedHeight
        : Math.min(rows.contentHeight, list.cap)

    onWantedChanged: list.reconcile()
    readonly property var navigation: host.navigation || null
    readonly property string registeredKey: host.model ? host.model.key : ""
    Component.onCompleted: { list.reconcile(); if (navigation) navigation.register(registeredKey, list) }
    Component.onDestruction: if (navigation) navigation.forget(registeredKey, list)
    readonly property var controlledSelection: Props.listSelected(host.model)
    onControlledSelectionChanged: list.applySelection()
    function applySelection() {
        if (controlledSelection === null) return
        list.selected = -1
        for (var i = 0; i < held.count; i++) if (list.valueOf(held.get(i)) === controlledSelection) { list.selected = i; break }
    }
    function valueOf(row) { var value = Props.selection_key(row.node); return value === null ? row.key : value }
    readonly property bool navigationReady: !navigation || navigation.ready(list)
    function move(direction) {
        if (!host.interactive || !navigationReady) return
        var index = selected
        for (var i = 0; i < held.count; i++) {
            index += direction
            if (index < 0 || index >= held.count) return
            var node = held.get(index).node
            if (!Props.disabled(node) && !Props.busy(node)) { selected = index; if (Props.bind(host.model, "select")) host.invoke("select", valueOf(held.get(index))); return }
        }
    }

    function reconcile() {
        var selectedKey = list.selected >= 0 && list.selected < held.count ? held.get(list.selected).key : null
        held.reconcile(list.wanted)

        list.selected = -1
        for (var index = 0; index < held.count; index++) {
            if (selectedKey !== null && held.get(index).key === selectedKey) {
                list.selected = index
                break
            }
        }
        list.applySelection()
    }

    function activate(index) {
        if (!list.host.interactive || !list.navigationReady || list.bound === null || index < 0 || index >= held.count) return
        var row = held.get(index)
        if (Props.disabled(row.node) || Props.busy(row.node)) return
        var value = Props.selection_key(row.node)
        list.host.invoke("activate", value === null ? row.key : value)
    }

    // Node payloads are QVariantMaps, including nested children and props.
    KeyedChildren { id: held }

    ListView {
        id: rows
        objectName: "choices"
        anchors.fill: parent
        model: held
        spacing: list.host.space(list.gap)
        clip: true
        // Only take keys when there is something to do with them; a list
        // nothing is bound to should not swallow the panel's navigation.
        activeFocusOnTab: true
        enabled: list.bound !== null

        Keys.onUpPressed: list.move(-1)
        Keys.onDownPressed: list.move(1)
        Keys.onReturnPressed: list.activate(list.selected)
        Keys.onEnterPressed: list.activate(list.selected)

        // Scroll the keyboard selection into view.
        onCurrentIndexChanged: rows.positionViewAtIndex(rows.currentIndex, ListView.Contain)
        currentIndex: list.selected

        delegate: Item {
            id: row
            required property int index
            required property var node

            width: rows.width
            implicitHeight: Math.max(list.host.space(36), child.item ? child.item.implicitHeight + list.host.space(12) : 0)

            // Keep keyboard selection and pointer hover feedback independent.
            Rectangle {
                anchors.fill: parent
                radius: list.host.radius
                visible: row.index === list.selected || point.containsMouse
                color: row.index === list.selected
                    ? list.host.chosenFill
                    : list.host.hoverFill
            }

            Loader {
                id: child
                // Incubate result subtrees between frames so filtering cannot block typing.
                asynchronous: true
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.leftMargin: list.host.space(8)
                anchors.rightMargin: list.host.space(8)
                anchors.verticalCenter: parent.verticalCenter

                Component.onCompleted: setSource("../ViewNode.qml", {
                    "model": Qt.binding(function() { return row.node }),
                    "session": Qt.binding(function() { return list.host.session }),
                    "navigation": Qt.binding(function() { return list.host.navigation || null }),
                    "theme": Qt.binding(function() { return list.host.theme }),
                    "assets": Qt.binding(function() { return list.host.assets }),
                    "foreground": Qt.binding(function() { return list.host.ink })
                })
            }

            MouseArea {
                id: point
                anchors.fill: parent
                hoverEnabled: true
                enabled: list.bound !== null && list.host.interactive && list.navigationReady && !Props.disabled(row.node) && !Props.busy(row.node)
                acceptedButtons: Qt.LeftButton
                z: -1
                onClicked: {
                    rows.forceActiveFocus()
                    list.selected = row.index
                    list.activate(row.index)
                }
            }
        }
    }
}
