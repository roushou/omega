import QtQuick
import QtTest
import "../core" as Core
import "../../../omega-omarchy/shell" as Omarchy

TestCase {
    id: test
    name: "PanelPresentation"
    Component { id: connectionFactory; Core.RendererConnection { plugin: "audio"; surface: "panel"; module: "audio" } }
    Component { id: panelFactory; Omarchy.PanelSession {} }
    property var link
    property var panel
    function init() {
        failOnWarning(/.*/)
        link = createTemporaryObject(connectionFactory, test)
        tryCompare(link, "connected", true)
        link.attached = true
        snapshot(false)
        panel = createTemporaryObject(panelFactory, test, { connection: link })
        link.socket.written = ""
    }
    function snapshot(shown) {
        link.receiveView({plugin:"audio",surface:"panel",instance:{id:"panel",incarnation:"test"},requested:shown ? 2 : 1,view:{revision:"1",root:{type:"text"}}})
    }
    function messages() { return link.socket.written.trim().split("\n").filter(line => line !== "").map(line => JSON.parse(line)) }
    function changes() { return messages().filter(frame => frame.invoke.changePresentation !== undefined) }
    function answer(index, error) {
        link.onLine(JSON.stringify({streamId:changes()[index].streamId,result:{done:true,error:error ? {message:error} : undefined}}))
    }
    function test_stale_hidden_snapshot_does_not_cancel_open() {
        panel.setOpen(true)
        compare(messages().length,1) // No premature visibility report.
        snapshot(false)
        compare(changes().length,1)
        snapshot(true)
        verify(panel.opened)
        answer(0)
        verify(!panel.pending)
        compare(changes().length,1)
    }
    function test_result_before_snapshot_retains_queued_close() {
        panel.toggle()
        answer(0)
        verify(panel.pending)
        panel.toggle()
        snapshot(false)
        compare(changes().length,1)
        snapshot(true)
        compare(changes().length,2)
        compare(changes()[1].invoke.changePresentation.action,"PRESENTATION_ACTION_HIDE")
        snapshot(false)
        answer(1)
        verify(!panel.opened)
        verify(!panel.pending)
    }
    function test_snapshot_before_result_coalesces_rapid_clicks() {
        panel.toggle()
        panel.toggle()
        panel.toggle()
        snapshot(true)
        verify(panel.pending)
        answer(0)
        verify(panel.opened)
        verify(!panel.pending)
        compare(changes().length,1)
    }
    function test_remote_hide_is_only_reported() {
        snapshot(true)
        snapshot(false)
        verify(!panel.opened)
        compare(changes().length,0)
        compare(messages().length,2)
    }
    function test_refusal_discards_queued_intent_and_allows_retry() {
        panel.setOpen(true)
        panel.setOpen(false)
        answer(0,"Presentation refused.")
        verify(!panel.pending)
        compare(panel.queued,null)
        compare(link.requests.error,"Presentation refused.")
        panel.setOpen(true)
        compare(changes().length,2)
    }
    function test_redundant_open_and_close_do_not_wait_for_a_deduplicated_snapshot() {
        panel.setOpen(false)
        compare(changes().length,0)
        snapshot(true)
        panel.setOpen(true)
        verify(!panel.pending)
        compare(changes().length,0)
    }
    function test_missing_snapshot_has_a_recovery_deadline() {
        panel.setOpen(true)
        answer(0)
        panel.deadline.interval = 1
        tryCompare(panel,"pending",false)
        verify(!panel.opened)
    }
    function test_disconnect_discards_pending_intent() {
        panel.setOpen(true)
        panel.setOpen(false)
        link.disconnected()
        verify(!panel.pending)
        verify(!panel.opened)
        compare(panel.queued,null)
    }
}
