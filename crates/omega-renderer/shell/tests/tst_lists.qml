import QtQuick
import QtTest
import "../core" as Renderer

TestCase {
    id: test
    name: "ListLoading"
    when: windowShown
    visible: true
    width: 500
    height: 400

    Component {
        id: factory
        Renderer.ViewNode { width: 450; height: 350 }
    }

    function field(key, value) {
        return {type: "field", key: key, props: {
            name: {stringValue: key}, value: {stringValue: value}
        }}
    }

    function model(children) {
        return {type: "list", key: "results", props: {
            height: {intValue: "300"}
        }, children: children}
    }

    function init() { failOnWarning(/.*/) }

    function test_pending_row_uses_latest_model() {
        var view = createTemporaryObject(factory, test, {
            model: model([field("entry", "old")])
        })
        // Row construction must yield before instantiating its controls.
        compare(findChild(view, "entry"), null)
        // Update before yielding to asynchronous delegate construction.
        view.model = model([field("entry", "current")])
        tryVerify(function() { return findChild(view, "entry") !== null })
        compare(findChild(view, "entry").text, "current")
    }

    function test_replaced_pending_rows_do_not_reappear() {
        var view = createTemporaryObject(factory, test, {
            model: model([field("removed", "old")])
        })
        view.model = model([])
        view.model = model([field("replacement", "new")])
        tryVerify(function() { return findChild(view, "replacement") !== null })
        compare(findChild(view, "removed"), null)
        compare(findChild(view, "replacement").text, "new")
    }

    function test_retained_row_preserves_local_draft_and_accepts_new_values() {
        var first = field("first", "original")
        var second = field("second", "second")
        var view = createTemporaryObject(factory, test, {model: model([first, second])})
        tryVerify(function() { return findChild(view, "first") !== null })
        var editor = findChild(view, "first")
        editor.text = "draft"
        view.model = model([second, first])
        compare(findChild(view, "first"), editor)
        compare(editor.text, "draft")
        view.model = model([second, field("first", "reset")])
        tryCompare(editor, "text", "reset")
    }
}
