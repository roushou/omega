import QtQuick
import QtTest
import "../core" as Core
import "../preview" as Previews

TestCase {
    id: test
    name: "PreviewLifetime"
    when: windowShown
    visible: true
    width: 400; height: 200
    Core.Theme { id: testTheme }
    Core.Assets { id: testAssets }
    QtObject {
        id: testSession
        property Core.Requests requests: Core.Requests {}
        property Core.Navigation navigation: Core.Navigation {}
        function press(bound, value, key, event) { return true }
    }
    Component { id: component; Previews.Viewport { width: 400; height: 200; theme: testTheme; assets: testAssets; session: testSession } }
    function init() { failOnWarning(/.*/) }
    function field(value, revision) {
        return {revision: String(revision), root: {key: "query", type: "field", props: {
            name: {stringValue: "query"}, value: {stringValue: value}, controlled: {boolValue: true},
            edit_revision: {intValue: "0"}, reset_revision: {intValue: "0"}
        }, children: [], events: {}}}
    }
    function test_new_epoch_discards_local_drafts_but_same_epoch_preserves_them() {
        var viewport = createTemporaryObject(component, test, {epoch: "1", view: field("initial", 1)})
        wait(1)
        var editor = findChild(viewport, "editor")
        verify(editor !== null)
        editor.forceActiveFocus()
        keyClick(Qt.Key_End)
        keyClick(Qt.Key_X)
        compare(editor.text, "initialx")
        viewport.view = field("initial", 2)
        wait(1)
        compare(findChild(viewport, "editor"), editor)
        compare(editor.text, "initialx")
        viewport.epoch = "2"
        viewport.view = field("reset", 3)
        wait(1)
        compare(findChild(viewport, "editor").text, "reset")
    }
}
