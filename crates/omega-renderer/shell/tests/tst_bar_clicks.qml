import QtQuick
import QtTest
import "../../../omega-omarchy/shell" as Omarchy

TestCase {
    id: test
    name: "BarClicks"
    when: windowShown
    visible: true
    width: 300
    height: 80

    Component {
        id: factory
        Item {
            id: slot
            width: widget.implicitWidth
            height: widget.implicitHeight
            property alias widget: widget
            property var targets: []
            function registerClickTarget(target) { targets = targets.concat([target]) }
            function unregisterClickTarget(target) { targets = targets.filter(item => item !== target) }
            Omarchy.BarWidget {
                id: widget
                anchors.fill: parent
                bar: slot
                settings: ({unit:"workspaces",surface:"indicator",module:"workspaces"})
            }
            // The host owns the pointer grab for module dragging and forwards clicks
            // to registered targets before propagating them to embedded controls.
            MouseArea {
                anchors.fill: parent
                propagateComposedEvents: true
                onClicked: mouse => {
                    for (var target of slot.targets) {
                        if (target.visible && target.pressable) {
                            target.triggerPress(mouse.button)
                            return
                        }
                    }
                    mouse.accepted = false
                }
            }
        }
    }
    function init() { failOnWarning(/.*/) }
    function indicator(disabled) {
        var slot = createTemporaryObject(factory, test)
        verify(slot !== null)
        var link = slot.widget.link
        tryCompare(link, "connected", true)
        link.attached = true
        link.onLine(JSON.stringify({unit:"workspaces",surface:"indicator",
            instance:{id:"workspace-instance",incarnation:"session"},requested:2,
            view:{revision:"7",root:{type:"button",key:"workspace-2",
                props:{label:{stringValue:"2"},flat:{boolValue:true},disabled:{boolValue:disabled},width:{intValue:"20"},height:{intValue:"24"}},
                events:{press:{command:"select",args:[{intValue:"2"}]}}}}}))
        tryVerify(() => slot.width > 20)
        wait(1)
        return slot
    }
    function test_workspace_click_reaches_its_retained_binding() {
        var slot = indicator(false)
        var link = slot.widget.link
        var before = link.socket.written.length
        mouseClick(slot, slot.width / 2, slot.height / 2)
        tryVerify(() => link.socket.written.length > before)
        var request = JSON.parse(link.socket.written.substring(before).trim())
        compare(request.invoke.interact.instance.id, "workspace-instance")
        compare(request.invoke.interact.revision, "7")
        compare(request.invoke.interact.node, "workspace-2")
        compare(request.invoke.interact.event, "press")
        // A pending action must not admit a duplicate click.
        before = link.socket.written.length
        mouseClick(slot, slot.width / 2, slot.height / 2)
        compare(link.socket.written.length, before)
    }
    function test_disabled_inner_control_does_not_dispatch() {
        var slot = indicator(true)
        var link = slot.widget.link
        var before = link.socket.written.length
        mouseClick(slot, slot.width / 2, slot.height / 2)
        compare(link.socket.written.length, before)
    }
    function test_panel_indicator_keeps_host_click_forwarding() {
        var slot = indicator(false)
        slot.widget.settings = {unit:"workspaces",surface:"indicator",module:"workspaces",panel:"panel"}
        var panel = slot.widget.panelLink
        tryCompare(panel, "connected", true)
        panel.attached = true
        panel.onLine(JSON.stringify({unit:"workspaces",surface:"panel",
            instance:{id:"panel-instance",incarnation:"session"},requested:1,
            view:{revision:"1",root:{type:"text",key:"panel"}}}))
        var before = panel.socket.written.length
        var indicatorBefore = slot.widget.link.socket.written.length
        mouseClick(slot, slot.width / 2, slot.height / 2)
        var request = JSON.parse(panel.socket.written.substring(before).trim())
        compare(request.invoke.changePresentation.instance.id, "panel-instance")
        compare(request.invoke.changePresentation.action, "PRESENTATION_ACTION_PRESENT")
        compare(slot.widget.link.socket.written.length, indicatorBefore)
    }
}
