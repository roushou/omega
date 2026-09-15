import QtQuick
import QtTest
import "../../../omega-omarchy/shell" as Omarchy

TestCase {
    id: test
    name: "PopupOwnership"
    property var activePopout: null
    Component {
        id: hostFactory
        QtObject {
            id: host
            property var controller
            readonly property bool opened: controller.owner === host && controller.opened
            function open() { controller.setOpen(true, host) }
            function close() { if (controller.owner === host) controller.setOpen(false) }
            onOpenedChanged: {
                // Omarchy's bar shares one coordinator across all monitor replicas.
                if (opened) {
                    if (test.activePopout && test.activePopout !== host) test.activePopout.close()
                    test.activePopout = host
                } else if (test.activePopout === host) {
                    test.activePopout = null
                }
            }
            Component.onCompleted: controller.registerHost(host)
            Component.onDestruction: controller.releaseHost(host)
        }
    }
    Component { id: leaseFactory; Omarchy.PlacementConnection { unit: "audio"; surface: "panel"; module: "audio" } }
    property var laptop
    property var external
    property var link
    property var controller
    function init() {
        failOnWarning(/.*/)
        activePopout = null
        var first = createTemporaryObject(leaseFactory, test)
        var second = createTemporaryObject(leaseFactory, test)
        compare(first.connection, second.connection)
        link = first.connection
        controller = link.panelSession
        tryCompare(link, "connected", true)
        link.attached = true
        snapshot(false)
        laptop = createTemporaryObject(hostFactory, test, {controller: controller})
        external = createTemporaryObject(hostFactory, test, {controller: controller})
        link.socket.written = ""
    }
    function snapshot(shown) {
        link.receiveView({unit:"audio",surface:"panel",instance:{id:"panel",incarnation:"test"},requested:shown ? 2 : 1,view:{revision:"1",root:{type:"text"}}})
    }
    function changes() {
        return link.socket.written.trim().split("\n").filter(line => line !== "")
            .map(line => JSON.parse(line)).filter(frame => frame.invoke.changePresentation !== undefined)
    }
    function answer(index) { link.onLine(JSON.stringify({streamId:changes()[index].streamId,result:{done:true}})) }
    function test_clicked_monitor_owns_popup_without_coordinator_closing_it() {
        external.open()
        snapshot(true)
        answer(0)
        verify(external.opened)
        verify(!laptop.opened)
        compare(activePopout, external)
        compare(changes().length, 1)
        verify(!controller.pending)
    }
    function test_transfer_between_monitors_does_not_hide_shared_instance() {
        laptop.open()
        snapshot(true)
        answer(0)
        controller.toggle(external)
        verify(external.opened)
        verify(!laptop.opened)
        laptop.close() // A stale native dismissal cannot close the new owner.
        compare(changes().length, 1)
        compare(activePopout, external)
        controller.toggle(external)
        compare(changes().length, 2)
        compare(changes()[1].invoke.changePresentation.action, "PRESENTATION_ACTION_HIDE")
    }
    function test_transfer_while_open_is_pending_preserves_single_transaction() {
        controller.toggle(laptop)
        controller.toggle(external)
        snapshot(true)
        answer(0)
        verify(external.opened)
        verify(!laptop.opened)
        compare(changes().length, 1)
        verify(!controller.pending)
    }
    function test_remote_present_selects_one_host() {
        snapshot(true)
        verify(laptop.opened)
        verify(!external.opened)
        compare(changes().length, 0)
        snapshot(false)
        verify(!laptop.opened)
        compare(changes().length, 0)
    }
    function test_removing_owner_hides_without_moving_popup() {
        external.open()
        snapshot(true)
        answer(0)
        controller.releaseHost(external)
        compare(controller.owner, null)
        verify(!laptop.opened)
        verify(!external.opened)
        compare(changes().length, 2)
        snapshot(false)
        answer(1)
        snapshot(true)
        verify(laptop.opened)
    }
    function test_removing_other_monitor_does_not_close_popup() {
        laptop.open()
        snapshot(true)
        answer(0)
        controller.releaseHost(external)
        verify(laptop.opened)
        compare(changes().length, 1)
    }
    function test_removing_owner_during_open_does_not_flash_on_other_monitor() {
        external.open()
        controller.releaseHost(external)
        snapshot(true)
        verify(!laptop.opened)
        verify(!external.opened)
        answer(0)
        compare(changes().length, 2)
        compare(changes()[1].invoke.changePresentation.action, "PRESENTATION_ACTION_HIDE")
        snapshot(false)
        answer(1)
        verify(!controller.pending)
    }
}
