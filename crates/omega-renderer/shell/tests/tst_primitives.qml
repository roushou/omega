import QtQuick
import QtTest
import "../core" as Renderer
import "../core/nodes" as Nodes

TestCase {
    id: test
    name: "Primitives"
    when: windowShown
    visible: true
    width: 400
    height: 500

    property var pressed: null
    property bool dismissed: false

    Renderer.Requests { id: requests }
    QtObject {
        id: connection
        readonly property var requests: requests
        function press(bound, value, key, event) {
            test.pressed = { value: value, event: event, key: key }
        }
    }

    Component {
        id: primitiveView
        Renderer.ViewNode { session: connection }
    }

    function test_new_node_kinds_render_without_warnings() {
        failOnWarning(/.*/)
        var models = [
            {type:"checkbox",key:"c",props:{on:{boolValue:true},label:{stringValue:"Auto"}},events:{change:{command:"set"}}},
            {type:"dropdown",key:"d",props:{selected:{stringValue:"auto"},placeholder:{stringValue:"Band"}},events:{select:{command:"set"}},children:[
                {type:"text",key:"auto",props:{text:{stringValue:"Auto"}}},
                {type:"text",key:"5",props:{text:{stringValue:"5 GHz"}}}
            ]},
            {type:"disclosure",key:"e",props:{title:{stringValue:"Advanced"},open:{boolValue:true}},events:{toggle:{command:"set"}},children:[
                {type:"text",key:"body",props:{text:{stringValue:"Details"}}}
            ]},
            {type:"dialog",key:"f",props:{title:{stringValue:"Confirm"},body:{stringValue:"Proceed?"},confirm:{stringValue:"Yes"},cancel:{stringValue:"No"}},events:{confirm:{command:"yes"},cancel:{command:"no"},dismiss:{command:"close"}}},
            {type:"badge",key:"g",props:{count:{intValue:"3"},hidden_when_zero:{boolValue:true}}},
            {type:"keycap",key:"h",props:{label:{stringValue:"Ctrl"}}},
            {type:"status",key:"i",props:{title:{stringValue:"Empty"},message:{stringValue:"None"},icon:{stringValue:"search"}}},
            {type:"scroll",key:"j",children:[
                {type:"text",key:"one",props:{text:{stringValue:"One"}}},
                {type:"text",key:"two",props:{text:{stringValue:"Two"}}}
            ]},
            {type:"textarea",key:"k",props:{rows:{intValue:"4"},label:{stringValue:"Notes"},placeholder:{stringValue:"Write"},controlled:{boolValue:true},edit_revision:{intValue:"0"},reset_revision:{intValue:"0"}},events:{change:{command:"text"}}}
        ]
        for (var i = 0; i < models.length; i++) {
            var view = createTemporaryObject(primitiveView, test, { model: models[i] })
            verify(view !== null)
            verify(view.implicitWidth >= 0)
            verify(view.implicitHeight >= 0)
        }
    }

    QtObject {
        id: checkboxHost
        property var model: ({props:{on:{boolValue:false}},events:{change:{command:"set"}}})
        property bool pending: false
        property bool interactive: true
        property color ink: "white"
        property color rule: "gray"
        property color trackFill: "gray"
        property var theme: ({ background: "black" })
        property string fontFamily: "sans-serif"
        property int captionSize: 12
        property int fontSize: 14
        function space(value) { return value }
        function invoke(bound, value) { test.pressed = value }
    }
    Nodes.Checkbox { id: checkbox; host: checkboxHost }

    function test_checkbox_reports_the_negated_state() {
        checkbox.activate()
        compare(test.pressed, true)
    }

    QtObject {
        id: dialogHost
        property var model: ({props:{title:{stringValue:"Confirm"},confirm:{stringValue:"Yes"},cancel:{stringValue:"No"}},events:{dismiss:{command:"close"}}})
        property bool interactive: true
        property color ink: "white"
        property color rule: "gray"
        property var theme: ({ background: "black" })
        property string fontFamily: "sans-serif"
        property int fontSize: 14
        property real radius: 3
        property int fixedWidth: 0
        function space(value) { return value }
        function typeSize(role, fallback) { return fallback }
        function invoke(bound, value) { test.dismissed = true }
    }
    Nodes.Dialog { id: dialog; host: dialogHost }

    function test_dialog_escape_dismisses_only_when_bound() {
        failOnWarning(/.*/)
        dialog.forceActiveFocus()
        keyClick(Qt.Key_Escape)
        compare(test.dismissed, true)

        test.dismissed = false
        var unbound = JSON.parse(JSON.stringify(dialogHost.model))
        delete unbound.events.dismiss
        dialogHost.model = unbound
        keyClick(Qt.Key_Escape)
        compare(test.dismissed, false)
    }

    QtObject {
        id: numericHost
        property var model: ({props:{numeric:{boolValue:true},min:{doubleValue:0},max:{doubleValue:150},step:{doubleValue:1}}})
        property bool pending: false
        property bool interactive: true
        property color ink: "white"
        property string fontFamily: "sans-serif"
        property int fontSize: 14
        property real radius: 3
        property var theme: ({ motion: false })
        function space(value) { return value }
        function typeSize(role, fallback) { return fallback }
        function controlFill(focused, hot) { return "gray" }
    }
    Nodes.Field { id: numericField; host: numericHost }

    function test_numeric_field_validates_range_and_step() {
        compare(numericField.numericValid("75"), true)
        compare(numericField.numericValid("0"), true)
        compare(numericField.numericValid("150"), true)
        compare(numericField.numericValid("200"), false)
        compare(numericField.numericValid("-1"), false)
        compare(numericField.numericValid("1.5"), false)
        compare(numericField.numericValid("abc"), false)
    }
}
