import QtQuick
import QtTest
import "../core" as Renderer

TestCase {
    id: test
    name: "ControlledEditing"
    when: windowShown
    visible: true
    width: 400
    height: 250
    property var calls: []
    Renderer.Requests { id: requestState }
    Renderer.Navigation { id: navigationState }
    QtObject {
        id: testSession
        property var requests: requestState
        property var navigation: navigationState
        function press(bound, value, key, event) { test.calls.push({value:value,event:event}); return requestState.begin(test.calls.length,key,Date.now()) }
    }
    Component { id: factory; Renderer.ViewNode { width: 350; session: testSession } }
    function model(text, revision, reset, binding) { return { type:"field",key:"query",props:{name:{stringValue:"query"},controlled:{boolValue:true},value:{stringValue:text},edit_revision:{intValue:String(revision)},reset_revision:{intValue:String(reset)},autofocus:{boolValue:true}},events:{change:{local:String(binding)}}} }
    function init() { calls = []; requestState.pending = ({}); requestState.error = ""; failOnWarning(/.*/) }
    function test_delayed_values_preserve_newer_typing_and_coalesce_pending_edits() {
        var view = createTemporaryObject(factory, test, {model:model("",0,0,1)})
        var field = findChild(view,"query")
        tryVerify(function() { return findChild(field,"editor").activeFocus })
        keyClick(Qt.Key_A)
        tryVerify(function() { return test.calls.length === 1 })
        keyClick(Qt.Key_B)
        compare(field.text,"ab")
        view.model = model("a",1,0,2)
        compare(field.text,"ab")
        requestState.finish(1,{done:true})
        tryVerify(function() { return test.calls.length === 2 })
        compare(test.calls[1].value.text,"ab")
        compare(test.calls[1].value.revision,2)
        view.model = model("reset",0,1,3)
        compare(field.text,"reset")
    }
    function test_programmatic_reset_does_not_emit_an_edit() {
        var view = createTemporaryObject(factory,test,{model:model("initial",0,0,1)})
        view.model = model("reset",0,1,2)
        wait(0)
        compare(test.calls.length,0)
        compare(findChild(view,"query").text,"reset")
    }
    function test_composition_defers_edits_and_applies_a_queued_reset_once() {
        var view = createTemporaryObject(factory,test,{model:model("",0,0,1)})
        var field = findChild(view,"query")
        field.composing = true
        field.text = "unfinished"
        field.edited()
        view.model = model("reset",0,1,2)
        compare(field.text,"unfinished")
        wait(0)
        compare(test.calls.length,0)
        field.composing = false
        compare(field.text,"reset")
        wait(0)
        compare(test.calls.length,0)
        field.composing = true
        field.text = "committed"
        field.composing = false
        tryVerify(function() { return test.calls.length === 1 })
        compare(test.calls[0].value.text,"committed")
        compare(test.calls[0].value.reset,1)
    }
    function test_navigation_waits_for_the_latest_edit_to_be_rendered() {
        var view = createTemporaryObject(factory, test, {model:model("",0,0,1)})
        var field = findChild(view,"query")
        tryVerify(function() { return findChild(field,"editor").activeFocus })
        keyClick(Qt.Key_A)
        tryVerify(function() { return test.calls.length === 1 })
        verify(!field.navigationReady)
        requestState.finish(1,{done:true})
        verify(!field.navigationReady)
        view.model = model("a",1,0,2)
        tryVerify(function() { return field.navigationReady })
    }
}
