import QtQuick
import QtTest
import "../core" as Renderer

TestCase {
    id: test
    name: "InstanceSessions"
    Renderer.Requests { id: tracker }
    QtObject {
        id: connection
        readonly property var requests: tracker
        property int stream: 1
        function interact(identity, revision, key, event, value) {
            var admitted = tracker.begin(stream, identity.id + "/" + key, 0)
            stream += 2
            return admitted
        }
    }
    Renderer.InstanceSession {
        id: first
        connection: connection
        snapshot: ({instance:{id:"first",incarnation:"test"},view:{revision:"1"}})
    }
    Renderer.InstanceSession {
        id: second
        connection: connection
        snapshot: ({instance:{id:"second",incarnation:"test"},view:{revision:"1"}})
    }
    SignalSpy { id: firstSettled; target: first.requests; signalName: "settled" }
    SignalSpy { id: secondSettled; target: second.requests; signalName: "settled" }
    function init() {
        failOnWarning(/.*/)
        tracker.disconnected()
        connection.stream = 1
        first.requests.error = ""
        second.requests.error = ""
        firstSettled.clear()
        secondSettled.clear()
    }
    function test_pending_feedback_and_completion_are_scoped_to_an_instance() {
        verify(first.press({command:"submit"},undefined,"form","submit"))
        verify(second.press({command:"submit"},undefined,"form","submit"))
        verify(first.requests.busy("form"))
        verify(second.requests.busy("form"))
        tracker.finish(1,{done:true,error:{message:"First refused"}})
        compare(first.requests.error,"First refused")
        compare(second.requests.error,"")
        compare(firstSettled.count,1)
        compare(firstSettled.signalArguments[0][0],"form")
        compare(secondSettled.count,0)
        verify(!first.requests.busy("form"))
        verify(second.requests.busy("form"))
        tracker.finish(3,{done:true,ok:{}})
        compare(secondSettled.count,1)
        compare(secondSettled.signalArguments[0][0],"form")
        compare(first.requests.error,"First refused")
    }
}
