import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../core"

Rectangle {
    id: fixture
    color: fixtureTheme.background
    property string scenario: "normal"
    property string lastInteraction: "Interactions are captured here; no desktop commands run."
    Theme { id: fixtureTheme }
    QtObject {
        id: capture
        readonly property QtObject requests: QtObject {
            function busy(key) { return false }
        }
        function press(bound, value, key) {
            fixture.lastInteraction = JSON.stringify({key:key, command:bound.command, value:value})
            return true
        }
    }
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16
        RowLayout {
            ComboBox {
                model: ["normal", "loading", "long labels", "disabled"]
                onCurrentTextChanged: fixture.scenario = currentText
            }
            Button {
                text: "Change theme"
                onClicked: fixtureTheme.accent = Qt.colorEqual(fixtureTheme.accent, "#93c5fd") ? "#c4b5fd" : "#93c5fd"
            }
        }
        Flickable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentHeight: view.implicitHeight
            clip: true
            ViewNode {
                id: view
                width: parent.width
                theme: fixtureTheme
                session: capture
                model: fixture.tree()
            }
        }
        Label {
            Layout.fillWidth: true
            text: fixture.lastInteraction
            color: fixtureTheme.muted
            wrapMode: Text.Wrap
        }
    }

    function tree() {
        var label = scenario === "long labels"
            ? "A desktop component with a very long label that must wrap within the window as you resize it"
            : "Desktop component"
        var children = [{type:"text",key:"title",props:{text:{stringValue:label},size:{stringValue:"title"}}}]
        if (scenario === "loading") {
            children.push({type:"text",key:"loading",props:{text:{stringValue:"Loading…"},emphasis:{stringValue:"muted"}}})
        } else {
            children.push({type:"slider",key:"brightness",props:{value:{doubleValue:0.65}},events:{change:{command:"set-brightness"}}})
            children.push({type:"toggle",key:"enabled",props:{on:{boolValue:true}},events:{change:{command:"set-enabled"}}})
            children.push({type:"button",key:"apply",props:{label:{stringValue:"Apply"},emphasis:{stringValue:"primary"}},events:{press:{command:"apply"}}})
        }
        return {type:"stack",key:"component",props:{align:{stringValue:"column"},gap:{intValue:"16"},disabled:{boolValue:scenario === "disabled"}},children:children}
    }
}
