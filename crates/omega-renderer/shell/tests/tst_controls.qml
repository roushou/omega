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


    QtObject {
        id: selectionHost
        property var model: ({})
        property bool interactive: true
        property color ink: "white"
        property color foreground: "white"
        property color chosenFill: "gray"
        property color trackFill: "gray"
        property color hoverFill: "gray"
        property string fontFamily: "sans-serif"
        property int fontSize: 14
        property real radius: 3
        property var connection: testConnection
        function space(value) { return value }
        function controlFill(focused, hot) { return "gray" }
        function invoke(bound, value) { test.submitted = value }
    }
    Component { id: groupFactory; Nodes.Group { host: selectionHost } }
    Component { id: listFactory; Nodes.List { host: selectionHost; width: 200; height: 100 } }

    function selectionModel(kind, key, value) {
        var props = {text:{stringValue:"Option"}}
        if (value !== null) props.selection_key = {stringValue:value}
        return {key:"instance/control",type:kind,
            props:{selected:{stringValue:value === null ? key : value}},
            events:{select:{command:"choose"},activate:{command:"choose"}},
            children:[{key:key,type:"text",props:props}]}
    }
    function test_scoped_choice_submits_domain_value_data() {
        return [
            {tag:"scoped", key:"instance/value", value:"value"},
            {tag:"empty", key:"instance/", value:""},
            {tag:"legacy", key:"original", value:null}
        ]
    }
    function test_scoped_choice_submits_domain_value(data) {
        selectionHost.model = selectionModel("group", data.key, data.value)
        var group = createTemporaryObject(groupFactory, test)
        verify(group !== null)
        var segment = null
        for (var i = 0; i < group.children.length; i++) {
            if (typeof group.children[i].activate === "function") segment = group.children[i]
        }
        verify(segment !== null)
        verify(segment.on)
        test.submitted = null
        segment.activate()
        compare(test.submitted, data.value === null ? data.key : data.value)
    }
    function test_scoped_list_submits_domain_value_data() {
        return test_scoped_choice_submits_domain_value_data()
    }
    function test_scoped_list_submits_domain_value(data) {
        selectionHost.model = selectionModel("list", data.key, data.value)
        var list = createTemporaryObject(listFactory, test)
        verify(list !== null)
        test.submitted = null
        list.activate(0)
        compare(test.submitted, data.value === null ? data.key : data.value)
    }

}
