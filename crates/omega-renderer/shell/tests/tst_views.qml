import QtQuick
import QtTest
import "../plugins/omega.view" as Renderer

TestCase {
    id: test
    name: "ViewUpdates"
    when: windowShown
    visible: true
    width: 400
    height: 300
    Renderer.ViewNode {
        id: view
        model: ({type:"stack",key:"root",children:[
            {type:"field",key:"draft",props:{name:{stringValue:"draft"},placeholder:{stringValue:"Before"}}},
            {type:"text",key:"title",props:{text:{stringValue:"Before"}}}
        ]})
    }
    function test_reorder_keeps_draft_and_updates_node_payload() {
        failOnWarning(/.*/)
        var draft = findChild(view, "draft")
        verify(draft !== null)
        draft.text = "Half typed"
        var next = JSON.parse(JSON.stringify(view.model))
        next.children.reverse()
        next.children[1].props.placeholder.stringValue = "After"
        view.model = next
        compare(findChild(view, "draft"), draft)
        compare(draft.text, "Half typed")
        compare(draft.host.model.props.placeholder.stringValue, "After")
    }
    property string activated: ""
    Renderer.Requests { id: listRequests }
    QtObject {
        id: listConnection
        readonly property var requests: listRequests
        function press(bound, value, key) { test.activated = value }
    }
    Renderer.ViewNode {
        id: listView
        y: 100
        connection: listConnection
        model: ({type:"list",key:"networks",props:{height:{intValue:"100"}},events:{activate:{command:"join"}},children:[
            {type:"text",key:"home",props:{text:{stringValue:"Home"}}},
            {type:"text",key:"office",props:{text:{stringValue:"Office"}}}
        ]})
    }
    function test_list_selection_follows_network_identity_after_reorder() {
        failOnWarning(/.*/)
        verify(listView.implicitWidth > 0)
        findChild(listView, "choices").forceActiveFocus()
        keyClick(Qt.Key_Down)
        var next = JSON.parse(JSON.stringify(listView.model))
        next.children.reverse()
        listView.model = next
        keyClick(Qt.Key_Return)
        compare(test.activated, "home")
    }

    Component {
        id: responsivePanel
        Renderer.ViewNode {
            width: 360
            model: ({type:"stack",key:"panel",props:{align:{stringValue:"column"}},children:[
                {type:"form",key:"join",props:{label:{stringValue:"Connect"}},children:[
                    {type:"field",key:"ssid",props:{name:{stringValue:"ssid"}}}
                ]}
            ]})
        }
    }
    function test_form_uses_panel_width_and_resizes_without_losing_draft() {
        failOnWarning(/.*/)
        var panel = createTemporaryObject(responsivePanel, test)
        verify(panel !== null)
        var field = findChild(panel, "ssid")
        verify(field !== null)
        tryCompare(field, "width", 360)
        field.text = "Home"
        panel.width = 200
        tryCompare(field, "width", 200)
        compare(field.text, "Home")
        verify(field.height >= 36)
    }

    Component {
        id: statusView
        Renderer.ViewNode {
            model: ({type:"button",key:"save",props:{label:{stringValue:"Save"},emphasis:{stringValue:"primary"},tone:{stringValue:"error"}}})
        }
    }
    function test_emphasis_does_not_override_status_or_disable_controls() {
        failOnWarning(/.*/)
        var button = createTemporaryObject(statusView, test)
        verify(button !== null)
        verify(Qt.colorEqual(button.ink, "red"))
        verify(button.interactive)
    }

}
