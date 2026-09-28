import QtQuick
import QtTest
import "../core" as Renderer

TestCase {
    id: test
    name: "Composition"
    when: windowShown
    visible: true
    width: 800
    height: 800

    property var calls: []
    Renderer.Requests { id: requestState }
    QtObject {
        id: testSession
        readonly property var requests: requestState
        function press(bound, value, key, event) {
            test.calls.push({binding: bound.local, value: value, key: key, event: event})
            return true
        }
    }
    Component { id: factory; Renderer.ViewNode { session: testSession } }

    function init() { calls = []; requestState.pending = ({}); requestState.error = ""; failOnWarning(/.*/) }
    function copy(value) { return JSON.parse(JSON.stringify(value)) }
    function view(model) {
        var result = createTemporaryObject(factory, test, {model: model})
        verify(result !== null)
        return result
    }
    function field(key) {
        return {type: "field", key: key, props: {
            name: {stringValue: key}, value: {stringValue: "initial"}
        }}
    }
    function editor(root, key) {
        var control = findChild(root, key)
        return control ? findChild(control, "editor") : null
    }
    function containers() {
        return ["stack", "grid", "scroll", "disclosure", "list"].map(function(kind) {
            return {tag: kind, kind: kind}
        })
    }
    function container(kind) {
        return {type: kind, key: "root", props: {
            open: {boolValue: true}, align: {stringValue: "column"}
        }, events: {activate: {local: "activate"}}, children: [field("first"), field("second")]}
    }

    function test_keyed_children_retain_drafts_on_payload_change_and_reorder_data() { return containers() }
    function test_keyed_children_retain_drafts_on_payload_change_and_reorder(data) {
        var root = view(container(data.kind))
        tryVerify(function() { return editor(root, "first") !== null && editor(root, "second") !== null })
        var first = editor(root, "first")
        var second = editor(root, "second")
        first.text = "unsent first"
        second.text = "unsent second"
        first.forceActiveFocus()
        var next = copy(root.model)
        next.children[0].props.help = {stringValue: "Updated help"}
        next.children[0].events = {submit: {local: "new-binding"}}
        next.children.reverse()
        next.children.splice(1, 0, field("added"))
        root.model = next
        tryVerify(function() { return editor(root, "added") !== null })
        compare(editor(root, "first"), first)
        compare(editor(root, "second"), second)
        compare(first.text, "unsent first")
        compare(second.text, "unsent second")
        verify(first.activeFocus)
        keyClick(Qt.Key_Return)
        compare(calls.length, 1)
        compare(calls[0].binding, "new-binding")
        compare(calls[0].value, "unsent first")
        next = copy(root.model)
        next.children.splice(0, 1)
        root.model = next
        tryVerify(function() { return editor(root, "second") === null })
        compare(editor(root, "first"), first)
    }

    function test_removed_or_changed_kind_ends_child_state_data() { return containers() }
    function test_removed_or_changed_kind_ends_child_state(data) {
        var root = view(container(data.kind))
        tryVerify(function() { return editor(root, "first") !== null })
        editor(root, "first").text = "old draft"
        var next = copy(root.model)
        next.children[0] = {type: "text", key: "first", props: {text: {stringValue: "Replacement"}}}
        root.model = next
        tryVerify(function() { return editor(root, "first") === null })
        next = copy(root.model)
        next.children[0] = field("first")
        root.model = next
        tryVerify(function() { return editor(root, "first") !== null })
        compare(editor(root, "first").text, "initial")
    }

    function test_disclosure_hiding_retains_child_draft() {
        var root = view(container("disclosure"))
        var input = editor(root, "first")
        input.text = "draft"
        var next = copy(root.model)
        next.props.open = {boolValue: false}
        root.model = next
        verify(!input.visible)
        next = copy(next)
        next.props.open = {boolValue: true}
        root.model = next
        compare(editor(root, "first"), input)
        verify(input.visible)
        compare(input.text, "draft")
    }

    function test_collapsed_controls_take_layout_space_data() {
        return [{tag: "dropdown", kind: "dropdown"}, {tag: "disclosure", kind: "disclosure"}]
    }
    function test_collapsed_controls_take_layout_space(data) {
        var root = view({type: "stack", key: "root", props: {align: {stringValue: "column"}}, children: [
            {type: data.kind, key: "control", props: {title: {stringValue: "Options"}}}, field("after")
        ]})
        var input = editor(root, "after")
        tryVerify(function() { return input.mapToItem(root, 0, 0).y >= root.space(32) })
        verify(root.implicitWidth > 0)
    }

    function test_scroll_uses_allocated_viewport_and_can_reach_content() {
        var root = view({type: "scroll", key: "scroll", props: {
            width: {intValue: "200"}, height: {intValue: "100"}, pad: {intValue: "5"}
        }, children: [{type: "spacer", key: "tall", props: {height: {intValue: "600"}, width: {intValue: "100"}}}]})
        var scroll = root.children[0].item
        tryCompare(scroll, "height", root.height - root.padding * 2)
        verify(scroll.contentHeight > scroll.height)
        scroll.contentY = scroll.contentHeight - scroll.height
        verify(scroll.atYEnd)
        root.height = 80
        compare(scroll.height, 80 - root.padding * 2)
    }

    function test_hidden_root_cannot_capture_another_roots_navigation() {
        var query = field("query")
        query.navigationTarget = "results"
        var model = {type: "stack", key: "root", children: [query,
            {type: "list", key: "results", events: {activate: {local: "activate"}}, children: [
                {type: "text", key: "one", props: {text: {stringValue: "One"}}}
            ]}
        ]}
        var shown = view(model)
        var hidden = view(model)
        hidden.visible = false
        var shownChoices = findChild(shown, "choices")
        var hiddenChoices = findChild(hidden, "choices")
        var input = editor(shown, "query")
        input.forceActiveFocus()
        keyClick(Qt.Key_Down)
        compare(shownChoices.currentIndex, 0)
        compare(hiddenChoices.currentIndex, -1)
        keyClick(Qt.Key_Return)
        compare(calls.length, 1)
        compare(calls[0].value, "one")
        hidden.destroy()
        wait(0)
        keyClick(Qt.Key_Return)
        compare(calls.length, 2)
    }

    function test_collapsed_disclosure_cannot_receive_navigation() {
        var query = field("query")
        query.navigationTarget = "results"
        var root = view({type: "stack", key: "root", children: [query,
            {type: "disclosure", key: "section", props: {open: {boolValue: true}}, children: [
                {type: "list", key: "results", events: {activate: {local: "activate"}}, children: [
                    {type: "text", key: "one", props: {text: {stringValue: "One"}}}
                ]}
            ]}
        ]})
        var choices = findChild(root, "choices")
        var input = editor(root, "query")
        var next = copy(root.model)
        next.children[1].props.open = {boolValue: false}
        root.model = next
        input.forceActiveFocus()
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        compare(choices.currentIndex, -1)
        compare(calls.length, 0)
        next = copy(next)
        next.children[1].props.open = {boolValue: true}
        root.model = next
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        compare(choices.currentIndex, 0)
        compare(calls.length, 1)
    }

    function test_multiline_editor_reveals_caret_and_clamps_after_reset() {
        var root = view({type: "textarea", key: "notes", props: {
            rows: {intValue: "2"}, value: {stringValue: "1\n2\n3\n4\n5\n6\n7\n8"}
        }})
        var input = findChild(root, "editor")
        input.forceActiveFocus()
        input.cursorPosition = input.length
        tryVerify(function() {
            var caret = input.cursorRectangle
            var position = input.mapToItem(root, caret.x, caret.y)
            return position.y >= 0 && position.y + caret.height <= root.height
        })
        verify(input.mapToItem(root, 0, 0).y < 0)
        input.cursorPosition = 0
        tryVerify(function() { return input.mapToItem(root, 0, 0).y >= 0 })
        input.cursorPosition = input.length
        var next = copy(root.model)
        next.props.value = {stringValue: "short"}
        root.model = next
        tryVerify(function() { return input.mapToItem(root, 0, 0).y >= 0 })
    }

    function test_multiline_controlled_draft_survives_pending_view_and_resets() {
        var root = view({type: "textarea", key: "notes", props: {
            controlled: {boolValue: true}, rows: {intValue: "2"},
            value: {stringValue: ""}, edit_revision: {intValue: "0"}, reset_revision: {intValue: "0"}
        }, events: {change: {local: "1"}}})
        var input = findChild(root, "editor")
        input.forceActiveFocus()
        keyClick(Qt.Key_A)
        tryCompare(test, "calls", [{binding: "1", value: {text: "a", revision: 1, reset: 0}, key: "notes", event: "change"}])
        verify(requestState.begin("pending", "notes", Date.now()))
        keyClick(Qt.Key_B)
        compare(input.text, "ab")
        compare(root.opacity, 1)
        var next = copy(root.model)
        next.props.value = {stringValue: "a"}
        next.props.edit_revision = {intValue: "1"}
        next.events.change = {local: "2"}
        root.model = next
        compare(input.text, "ab")
        requestState.finish("pending", {done: true})
        tryVerify(function() { return calls.length === 2 })
        compare(calls[1].value.text, "ab")
        next = copy(next)
        next.props.value = {stringValue: "reset"}
        next.props.reset_revision = {intValue: "1"}
        next.props.edit_revision = {intValue: "0"}
        root.model = next
        compare(input.text, "reset")
        wait(0)
        compare(calls.length, 2)
    }

    function test_viewport_accepts_late_content_and_retains_only_matching_identity() {
        var root = view({type: "viewport", key: "viewport", props: {
            height: {intValue: "200"}, width: {intValue: "300"}
        }, children: []})
        var next = copy(root.model)
        next.children = [field("first")]
        root.model = next
        tryVerify(function() { return editor(root, "first") !== null })
        var input = editor(root, "first")
        input.text = "draft"
        next = copy(next)
        next.children[0].props.help = {stringValue: "Updated"}
        root.model = next
        compare(editor(root, "first"), input)
        compare(input.text, "draft")
        next = copy(next)
        next.children = [field("replacement")]
        root.model = next
        compare(editor(root, "replacement").text, "initial")
        next = copy(next)
        next.children = []
        root.model = next
        tryVerify(function() { return editor(root, "replacement") === null })
    }
}
