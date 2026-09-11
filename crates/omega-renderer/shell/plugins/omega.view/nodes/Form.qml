import QtQuick
import "../Props.js" as Props

// The repeat model contains only field identities; value updates preserve drafts.
Item {
    id: rootForm
    required property var host
    implicitWidth: host.space(240)
    implicitHeight: contents.implicitHeight
    property var fields: Props.children(host.model)
    property var names: []
    property string validation: ""

    function sync() {
        var wanted = []
        for (var i = 0; i < rootForm.fields.length; i++) wanted.push(Props.fieldName(rootForm.fields[i]))
        if (JSON.stringify(wanted) !== JSON.stringify(rootForm.names)) rootForm.names = wanted
    }
    onFieldsChanged: sync()
    Component.onCompleted: sync()

    function submit() {
        if (!rootForm.host.interactive) return
        var plain = Object.create(null)
        for (var i = 0; i < entries.count; i++) {
            var field = entries.itemAt(i)
            var name = rootForm.names[i]
            if (!name || Object.prototype.hasOwnProperty.call(plain, name)) {
                rootForm.validation = "Form fields need unique names."
                return
            }
            plain[name] = field.text
        }
        rootForm.validation = ""
        rootForm.host.invoke(Props.bind(rootForm.host.model, "submit"), plain)
    }

    Connections {
        target: rootForm.host.connection ? rootForm.host.connection.requests : null
        function onSettled(key, success) {
            if (key !== rootForm.host.model.key || !success) return
            for (var i = 0; i < entries.count; i++) {
                if (Props.fieldSecret(rootForm.fields[i])) entries.itemAt(i).text = ""
            }
        }
    }

    Column {
        id: contents
        width: rootForm.width
        spacing: rootForm.host.space(10)
        Repeater {
            id: entries
            model: rootForm.names
            delegate: Field {
                id: field
                required property int index
                width: rootForm.width
                host: QtObject {
                    readonly property var model: rootForm.fields[field.index]
                    readonly property var form: rootForm
                    readonly property bool interactive: rootForm.host.interactive
                    readonly property color ink: rootForm.host.ink
                    readonly property color foreground: rootForm.host.foreground
                    readonly property string fontFamily: rootForm.host.fontFamily
                    readonly property int fontSize: rootForm.host.fontSize
                    readonly property real radius: rootForm.host.radius
                    function space(value) { return rootForm.host.space(value) }
                    function controlFill(focused, hot) { return rootForm.host.controlFill(focused, hot) }
                }
            }
        }

        Rectangle {
            id: submitButton
            width: rootForm.width
            height: Math.max(rootForm.host.space(36), label.implicitHeight + rootForm.host.space(16))
            radius: rootForm.host.radius
            color: rootForm.host.chosenFill
            activeFocusOnTab: true
            enabled: Props.bind(rootForm.host.model, "submit") !== null
            border.width: submitButton.activeFocus ? rootForm.host.space(2) : 0
            border.color: rootForm.host.ink
            Keys.onReturnPressed: rootForm.submit()
            Keys.onEnterPressed: rootForm.submit()
            Keys.onSpacePressed: rootForm.submit()
            Text {
                id: label
                anchors.centerIn: parent
                text: rootForm.host.pending ? "Working…" : Props.formLabel(rootForm.host.model)
                color: rootForm.host.ink
                font.family: rootForm.host.fontFamily
                font.pixelSize: rootForm.host.fontSize
            }
            MouseArea {
                anchors.fill: parent
                enabled: rootForm.host.interactive
                cursorShape: Qt.PointingHandCursor
                onClicked: { submitButton.forceActiveFocus(); rootForm.submit() }
            }
        }
        Text { text: rootForm.validation; visible: text !== ""; color: rootForm.host.ink }
    }
}
