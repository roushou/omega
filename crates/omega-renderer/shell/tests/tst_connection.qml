import QtQuick
import QtTest
import "../plugins/omega.view" as Renderer

TestCase {
    name: "ConnectionRecovery"
    Component { id: factory; Renderer.Connection { unit: "audio"; surface: "panel" } }
    function init() { failOnWarning(/.*/) }
    function connection() {
        var link = createTemporaryObject(factory, this)
        verify(link !== null)
        tryCompare(link, "connected", true)
        return link
    }
    function phase(link, phase, detail) {
        link.onLine(JSON.stringify({topic:"units",units:{units:[{unit:"audio",phase:phase,detail:detail || ""}]}}))
    }
    function test_lifecycle_status_does_not_mistake_empty_content_for_failure() {
        var link = connection()
        phase(link, "UNIT_PHASE_STARTING")
        compare(link.status, "Starting plugin…")
        phase(link, "UNIT_PHASE_RUNNING")
        link.onLine(JSON.stringify({unit:"audio",surface:"panel",view:{}}))
        compare(link.tree, null)
        compare(link.status, "")
        phase(link, "UNIT_PHASE_RESTARTING")
        compare(link.status, "Restarting plugin…")
        phase(link, "UNIT_PHASE_FAILED", "Executable missing")
        compare(link.status, "Executable missing")
    }
    function test_disconnect_clears_view_and_pending_work_then_recovers() {
        var link = connection()
        phase(link, "UNIT_PHASE_RUNNING")
        link.onLine(JSON.stringify({unit:"audio",surface:"panel",view:{root:{type:"text",key:"volume"}}}))
        verify(link.press({command:"volume"}, 0.5, "slider"))
        verify(link.requests.busy("slider"))
        link.disconnected()
        compare(link.tree, null)
        compare(link.drawnBy, "")
        verify(!link.requests.busy("slider"))
        verify(link.requests.error.indexOf("unknown") >= 0)
        compare(link.status, "Waiting for Omega…")
        link.connected = true
        phase(link, "UNIT_PHASE_RUNNING")
        link.onLine(JSON.stringify({unit:"audio",surface:"panel",view:{root:{type:"text",key:"volume"}}}))
        compare(link.status, "")
        verify(link.press({command:"volume"}, 0.4, "slider"))
        compare(link.requests.error, "")
    }
    function test_timeout_preserves_its_explanation_across_disconnect() {
        var link = connection()
        link.requests.begin(3, "connect", 0)
        verify(link.requests.expire(30000))
        var message = link.requests.error
        link.disconnected(message)
        compare(link.requests.error, message)
        verify(!link.requests.busy("connect"))
        link.disconnected()
        compare(link.requests.error, message)
    }
    function test_unsent_command_is_rejected_without_pending_state() {
        var link = connection()
        link.disconnected()
        verify(!link.press({command:"volume"}, 0.5, "slider"))
        verify(!link.requests.busy("slider"))
        verify(link.requests.error.indexOf("not sent") >= 0)
    }
}
