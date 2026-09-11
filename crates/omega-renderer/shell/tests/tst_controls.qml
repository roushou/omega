import QtQuick
import QtTest
import "../plugins/omega.view/nodes" as Nodes
import "../plugins/omega.view" as Renderer
import "../plugins/omega.view/Props.js" as Props

TestCase {
    id: test
    name: "Controls"
    when: windowShown
    visible: true
    width: 400
    height: 400
    property var submitted: null
    Renderer.Requests { id: requestState }
    QtObject {
        id: testConnection
        readonly property var requests: requestState
    }
    QtObject {
        id: host
        property var model: ({key:"form",type:"form",props:{label:{stringValue:"Connect"}},events:{submit:{command:"connect"}},children:[
            {type:"field",props:{name:{stringValue:"ssid"},placeholder:{stringValue:"Network"}}},
            {type:"field",props:{name:{stringValue:"password"},secret:{boolValue:true}}}
        ]})
        property var connection: testConnection
        property bool pending: false
        property bool interactive: !pending
        property color ink: "white"
        property color foreground: "white"
        property color chosenFill: "gray"
        property string fontFamily: "sans-serif"
        property int fontSize: 14
        property real radius: 3
        function space(value) { return value }
        function controlFill(focused, hot) { return "gray" }
        function invoke(bound, value) { test.submitted = value }
    }
    Nodes.Form { id: form; host: host }
    function test_form_submits_both_fields_and_retains_draft_on_update() {
        var ssid = findChild(form, "ssid")
        var password = findChild(form, "password")
        verify(ssid !== null)
        verify(password !== null)
        ssid.text = "Home"
        password.text = "secret"
        var changed = JSON.parse(JSON.stringify(host.model))
        changed.props.label.stringValue = "Join"
        host.model = changed
        compare(ssid.text, "Home")
        compare(password.text, "secret")
        form.submit()
        compare(test.submitted.ssid, "Home")
        compare(test.submitted.password, "secret")
        var encoded = Props.encode(test.submitted)
        compare(encoded.map.entries.ssid.stringValue, "Home")
        host.pending = true
        test.submitted = null
        form.submit()
        compare(test.submitted, null)
        host.pending = false
        requestState.settled("form", false)
        compare(password.text, "secret")
        requestState.settled("form", true)
        compare(password.text, "")
        compare(ssid.text, "Home")
    }
    QtObject {
        id: sliderHost
        property var model: ({props:{value:{doubleValue:0.25}},events:{change:{command:"volume"}}})
        property bool pending: false
        property bool interactive: !pending
        property color ink: "white"
        property color trackFill: "gray"
        function space(value) { return value }
        function pill(value) { return value / 2 }
        function invoke(bound, value) { test.submitted = value; pending = true }
    }
    Nodes.Slider { id: slider; host: sliderHost; y: 250; width: 100; height: 20 }
    function test_slider_submits_on_release_and_holds_while_pending() {
        failOnWarning(/.*/)
        verify(slider.implicitHeight >= 32)
        test.submitted = null
        mousePress(slider, 75, 10)
        compare(test.submitted, null)
        mouseRelease(slider, 75, 10)
        compare(test.submitted, 0.75)
        compare(slider.shown, 0.75)
        sliderHost.pending = false
        compare(slider.shown, 0.25)
        sliderHost.model = {props:{value:{doubleValue:0.75}},events:{change:{command:"volume"}}}
        compare(slider.shown, 0.75)
    }

}
