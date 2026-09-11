import QtQuick
import QtTest
import "../plugins/omega.view" as Renderer

TestCase {
    id: test
    name: "KeyboardControls"
    when: windowShown
    visible: true
    width: 500
    height: 700
    property var calls: []
    property bool accept: true
    property int stream: 1
    property bool escaped: false
    Keys.onEscapePressed: test.escaped = true
    Renderer.Requests { id: requestTracker }
    QtObject {
        id: testConnection
        readonly property var requests: requestTracker
        function press(bound, value, key) {
            if (!test.accept) return false
            if (!requestTracker.begin(test.stream, key, 0)) return false
            test.calls = test.calls.concat([{command:bound.command,value:value}])
            test.stream += 2
            return true
        }
    }
    Component {
        id: factory
        Renderer.ViewNode { width: 400; connection: testConnection }
    }
    function init() {
        failOnWarning(/.*/)
        requestTracker.disconnected()
        test.calls = []
        test.accept = true
        test.stream = 1
        test.escaped = false
    }
    function node(model) {
        var view = createTemporaryObject(factory, test, {model:model})
        verify(view !== null)
        view.forceActiveFocus()
        return view
    }
    function typeText(text) {
        for (var i = 0; i < text.length; ++i) keyClick(text[i])
    }
    function button(key, disabled) {
        return {type:"button",key:key,props:{label:{stringValue:key},disabled:{boolValue:!!disabled}},events:{press:{command:key}}}
    }
    function test_tab_skips_disabled_descendants_and_enter_activates() {
        var view = node({type:"stack",key:"root",props:{align:{stringValue:"column"}},children:[
            {type:"stack",key:"disabled",props:{disabled:{boolValue:true}},children:[button("forbidden",false)]},
            button("first",false), button("second",false)
        ]})
        keyClick(Qt.Key_Tab)
        keyClick(Qt.Key_Enter)
        compare(test.calls.length, 1)
        compare(test.calls[0].command, "first")
        requestTracker.finish(1,{done:true,error:{message:"Try again"}})
        keyClick(Qt.Key_Space)
        compare(test.calls.length, 2)
        compare(test.calls[1].command, "first")
        keyClick(Qt.Key_Tab)
        keyClick(Qt.Key_Return)
        compare(test.calls[2].command, "second")
    }
    function test_toggle_refused_admission_does_not_latch_optimistic_state() {
        var view = node({type:"toggle",key:"mute",props:{on:{boolValue:false}},events:{change:{command:"mute"}}})
        keyClick(Qt.Key_Tab)
        test.accept = false
        keyClick(Qt.Key_Space)
        compare(test.calls.length, 0)
        test.accept = true
        keyClick(Qt.Key_Space)
        compare(test.calls[0].value, true)
        requestTracker.finish(1,{done:true,error:{message:"Audio unavailable"}})
        keyClick(Qt.Key_Return)
        compare(test.calls[1].value, true)
    }
    function test_slider_blocks_duplicate_input_and_retries_after_refusal() {
        var view = node({type:"slider",key:"volume",props:{value:{doubleValue:0.5}},events:{change:{command:"volume"}}})
        keyClick(Qt.Key_Tab)
        keyClick(Qt.Key_Right)
        compare(test.calls[0].value, 0.55)
        keyClick(Qt.Key_Right)
        compare(test.calls.length, 1)
        requestTracker.finish(1,{done:true,error:{message:"Device unavailable"}})
        keyClick(Qt.Key_Home)
        compare(test.calls[1].value, 0)
        requestTracker.finish(3,{done:true})
        keyClick(Qt.Key_End)
        compare(test.calls[2].value, 1)
    }
    function test_choice_focus_survives_state_updates() {
        var model = {type:"group",key:"profile",events:{select:{command:"profile"}},children:[
            {type:"text",key:"balanced",props:{text:{stringValue:"Balanced"}}},
            {type:"text",key:"saver",props:{text:{stringValue:"Saver"}}}
        ]}
        var view = node(model)
        keyClick(Qt.Key_Tab)
        keyClick(Qt.Key_Tab)
        keyClick(Qt.Key_Space)
        compare(test.calls[0].value, "saver")
        requestTracker.finish(1,{done:true})
        var next = JSON.parse(JSON.stringify(model))
        next.props = {selected:{stringValue:"saver"}}
        view.model = next
        keyClick(Qt.Key_Enter)
        compare(test.calls[1].value, "saver")
    }

    function test_form_keyboard_retry_retains_drafts_and_clears_only_secrets_on_success() {
        var view = node({type:"form",key:"connect",props:{label:{stringValue:"Connect"}},events:{submit:{command:"connect"}},children:[
            {type:"field",key:"ssid",props:{name:{stringValue:"ssid"},label:{stringValue:"Network"}}},
            {type:"field",key:"password",props:{name:{stringValue:"password"},secret:{boolValue:true}}}
        ]})
        keyClick(Qt.Key_Tab)
        typeText("home")
        keyClick(Qt.Key_Tab)
        typeText("secret")
        keyClick(Qt.Key_Return)
        compare(test.calls.length, 1)
        compare(test.calls[0].value.ssid, "home")
        compare(test.calls[0].value.password, "secret")
        verify(JSON.stringify(requestTracker.pending).indexOf("secret") < 0)
        typeText("ignored")
        keyClick(Qt.Key_Return)
        compare(test.calls.length, 1)
        requestTracker.disconnected()
        keyClick(Qt.Key_Return)
        compare(test.calls.length, 2)
        compare(test.calls[1].value.password, "secret")
        requestTracker.finish(3,{done:true,error:{message:"Could not connect"}})
        keyClick(Qt.Key_Return)
        compare(test.calls.length, 3)
        compare(test.calls[2].value.password, "secret")
        requestTracker.finish(5,{done:true})
        compare(findChild(view, "password").text, "")
        compare(findChild(view, "ssid").text, "home")
        keyClick(Qt.Key_Escape)
        verify(test.escaped)
    }
    function test_form_tab_skips_disabled_and_busy_fields() {
        var view = node({type:"form",key:"edit",events:{submit:{command:"save"}},children:[
            {type:"field",key:"disabled",props:{name:{stringValue:"disabled"},disabled:{boolValue:true}}},
            {type:"field",key:"busy",props:{name:{stringValue:"busy"},busy:{boolValue:true}}},
            {type:"field",key:"editable",props:{name:{stringValue:"editable"}}}
        ]})
        keyClick(Qt.Key_Tab)
        typeText("hello")
        keyClick(Qt.Key_Return)
        compare(test.calls[0].value.editable, "hello")
        compare(test.calls[0].value.disabled, "")
        compare(test.calls[0].value.busy, "")
    }
    function test_list_keyboard_activation_respects_pending_and_disabled_rows() {
        var view = node({type:"list",key:"list",events:{activate:{command:"join"}},children:[
            {type:"text",key:"unavailable",props:{text:{stringValue:"Unavailable"},disabled:{boolValue:true}}},
            {type:"text",key:"home",props:{text:{stringValue:"Home"}}}
        ]})
        keyClick(Qt.Key_Tab)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        compare(test.calls.length, 0)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        compare(test.calls[0].value, "home")
        keyClick(Qt.Key_Return)
        compare(test.calls.length, 1)
        requestTracker.finish(1,{done:true,error:{message:"Unavailable"}})
        keyClick(Qt.Key_Enter)
        compare(test.calls.length, 2)
    }
}
