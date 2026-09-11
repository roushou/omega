import QtQuick
import QtTest
import "../plugins/omega.view"

TestCase {
    name: "CommandFeedback"
    Requests { id: requests }
    function init() { requests.disconnected(); requests.error = "" }
    function test_terminal_result_and_refusal() {
        verify(requests.begin(1, "volume", 0))
        verify(!requests.begin(3, "volume", 0))
        requests.finish(1, { done: false })
        verify(requests.busy("volume"))
        requests.finish(99, { done: true })
        verify(requests.busy("volume"))
        requests.finish(1, { done: true, error: { message: "Device disappeared" } })
        verify(!requests.busy("volume"))
        compare(requests.error, "Device disappeared")
    }
    function test_timeout_retains_uncertain_work_until_disconnect() {
        verify(requests.begin(1, "connect", 0))
        verify(!requests.expire(29999))
        verify(requests.expire(30000))
        verify(requests.busy("connect"))
        requests.disconnected()
        verify(!requests.busy("connect"))
        verify(requests.error.indexOf("unknown") >= 0)
    }
    function test_capacity_and_independent_controls() {
        for (var i = 0; i < 16; i++) verify(requests.begin(i * 2 + 1, "control" + i, 0))
        verify(!requests.begin(33, "extra", 0))
        requests.finish(1, { done: true })
        verify(requests.begin(33, "extra", 1))
    }
}
