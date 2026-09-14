import QtQuick
import "../Props.js" as Props

// Draft edits stay local; edit/reset generations protect them from delayed trees.
Item {
    id: field
    objectName: Props.fieldName(host.model)
    required property var host
    enabled: !Props.disabled(host.model) && !Props.busy(host.model)

    readonly property var bound: Props.bind(host.model, "submit")
    readonly property string given: Props.fieldValue(host.model)
    property alias text: input.text
    readonly property bool secret: Props.fieldSecret(host.model)

    readonly property bool controlled: Props.fieldControlled(host.model)
    property bool composing: input.inputMethodComposing
    onComposingChanged: if (!composing) {
        if (Props.fieldReset_revision(host.model) > resetRevision) synchronize()
        else edited()
    }
    property var lastGiven: null
    readonly property bool requestPending: !!host.pending
    onRequestPendingChanged: flush.restart()
    property int editRevision: 0
    property int resetRevision: 0
    property bool queuedEdit: false
    readonly property var navigation: host.session && host.session.navigation ? host.session.navigation : null
    readonly property bool navigationResolved: !!(host.model && host.model.navigationTarget)
    readonly property string navigationTarget: navigationResolved ? host.model.navigationTarget : Props.fieldNavigation(host.model)
    readonly property string registeredKey: host.model && host.model.key ? host.model.key : ""
    readonly property bool navigationReady: !composing && !queuedEdit && !host.pending && (!controlled || (Props.fieldEdit_revision(host.model) >= editRevision && Props.fieldReset_revision(host.model) === resetRevision))
    Component.onDestruction: if (navigation) navigation.forgetSource(registeredKey, field)
    property var sentBinding: null
    function synchronize() {
        if (field.composing) return
        if (!controlled) { if (lastGiven !== given) input.text = given; lastGiven = given; return }
        var reset = Props.fieldReset_revision(host.model)
        var revision = Props.fieldEdit_revision(host.model)
        if (reset > resetRevision || revision >= editRevision) {
            if (reset > resetRevision) { queuedEdit = false; sentBinding = null }
            resetRevision = reset
            editRevision = revision
            if (input.text !== given) input.text = given
        }
        flush.restart()
    }
    function edited() {
        if (field.composing || input.readOnly) return
        editRevision += 1
        queuedEdit = true
        flush.restart()
    }
    Timer {
        id: flush
        interval: 0
        onTriggered: {
            var bound = Props.bind(field.host.model, "change")
            if (!field.queuedEdit || !bound || field.host.pending || field.composing) return
            if (field.sentBinding !== null && bound.local === field.sentBinding) return
            var value = { text: input.text, revision: field.editRevision, reset: field.resetRevision }
            if (field.host.invoke("change", value)) { field.queuedEdit = false; field.sentBinding = bound.local || null }
        }
    }
    Connections {
        target: field.host
        function onModelChanged() { field.synchronize() }
    }
    onGivenChanged: synchronize()
    Component.onCompleted: { if (navigation) navigation.registerSource(registeredKey, field); synchronize(); if (Props.fieldAutofocus(host.model)) Qt.callLater(function() { input.forceActiveFocus() }) }

    implicitWidth: host.space(240)
    implicitHeight: labelHeight + Math.max(host.space(36), input.implicitHeight + host.space(16)) + helpHeight
    readonly property real labelHeight: fieldLabel.visible ? fieldLabel.implicitHeight + host.space(6) : 0
    readonly property real helpHeight: helpText.visible ? helpText.implicitHeight + host.space(6) : 0
    Text {
        id: helpText
        anchors.bottom: parent.bottom
        width: parent.width
        text: Props.fieldHelp(field.host.model)
        visible: text !== ""
        wrapMode: Text.Wrap
        color: field.host.ink
        opacity: 0.65
        font.family: field.host.fontFamily
        font.pixelSize: field.host.fontSize
    }
    Text {
        id: fieldLabel
        width: parent.width
        text: Props.fieldLabel(field.host.model)
        visible: text !== ""
        wrapMode: Text.Wrap
        color: field.host.ink
        font.family: field.host.fontFamily
        font.pixelSize: field.host.fontSize
    }
    Rectangle {
        anchors.fill: parent
        anchors.topMargin: field.labelHeight
        anchors.bottomMargin: field.helpHeight
        radius: host.radius
        // Through the kit's own function, so a focused field here looks like a
        // focused field anywhere else in the shell.
        color: field.host.controlFill(input.activeFocus, hover.containsMouse)

        HoverHandler { id: hover }

        TextInput {
            id: input
            cursorVisible: activeFocus && field.host.theme.motion
            objectName: "editor"
            anchors.fill: parent
            anchors.leftMargin: field.host.space(10)
            anchors.rightMargin: field.host.space(10)
            verticalAlignment: TextInput.AlignVCenter
            clip: true
            color: field.host.ink
            font.family: field.host.fontFamily
            font.pixelSize: field.host.fontSize
            selectByMouse: true
            activeFocusOnTab: true
            readOnly: (field.controlled ? !field.enabled : !field.host.interactive) || (field.host.form && !field.host.form.host.interactive)
            echoMode: field.secret ? TextInput.Password : TextInput.Normal

            onTextEdited: field.edited()
            Accessible.role: Accessible.EditableText
            Accessible.name: fieldLabel.text
            Keys.onUpPressed: event => { if (!field.composing && field.host.session && field.host.session.navigation && field.navigationTarget) field.host.session.navigation.move(field.host.model.key, field.navigationTarget, -1); else event.accepted = false }
            Keys.onDownPressed: event => { if (!field.composing && field.host.session && field.host.session.navigation && field.navigationTarget) field.host.session.navigation.move(field.host.model.key, field.navigationTarget, 1); else event.accepted = false }
            onAccepted: {
                if (field.host.session && field.host.session.navigation && field.navigationTarget) { if (!field.navigationReady) return; field.host.session.navigation.activate(field.host.model.key, field.navigationTarget); return }
                if (input.readOnly) return
                if (field.host.form) { field.host.form.submit(); return }
                if (field.bound === null) return
                field.host.invoke("submit", input.text)
            }

            Text {
                anchors.fill: parent
                verticalAlignment: Text.AlignVCenter
                text: Props.fieldPlaceholder(field.host.model)
                color: Qt.darker(field.host.ink, 1.4)
                font.family: field.host.fontFamily
                font.pixelSize: field.host.fontSize
                visible: input.text === ""
            }
        }
    }
}
