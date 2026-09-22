import QtQuick
import "../Props.js" as Props

// Multi-line draft edits stay local; edit/reset generations protect them from
// delayed trees, exactly like a single-line field.
Item {
    id: area
    required property var host
    enabled: !Props.disabled(host.model) && !Props.busy(host.model)

    readonly property string given: Props.textareaValue(host.model)
    property alias text: input.text
    readonly property int textSize: host.fontSize
    readonly property int rows: Math.max(1, Props.textareaRows(host.model))

    readonly property bool controlled: Props.textareaControlled(host.model)
    property bool composing: input.inputMethodComposing
    onComposingChanged: if (!composing) {
        if (Props.textareaReset_revision(host.model) > resetRevision) synchronize()
        else edited()
    }
    property var lastGiven: null
    readonly property bool requestPending: !!host.pending
    onRequestPendingChanged: flush.restart()
    property int editRevision: 0
    property int resetRevision: 0
    property bool queuedEdit: false
    property var sentBinding: null
    // A programmatic text write is not a user edit.
    property bool setting: false
    function apply(value) {
        if (input.text === value) return
        area.setting = true
        input.text = value
        area.setting = false
    }

    function synchronize() {
        if (area.composing) return
        if (!controlled) { if (lastGiven !== given) area.apply(given); lastGiven = given; return }
        var reset = Props.textareaReset_revision(host.model)
        var revision = Props.textareaEdit_revision(host.model)
        if (reset > resetRevision || revision >= editRevision) {
            if (reset > resetRevision) { queuedEdit = false; sentBinding = null }
            resetRevision = reset
            editRevision = revision
            area.apply(given)
        }
        flush.restart()
    }
    function edited() {
        if (area.composing || input.readOnly) return
        editRevision += 1
        queuedEdit = true
        flush.restart()
    }
    Timer {
        id: flush
        interval: 0
        onTriggered: {
            var bound = Props.bind(area.host.model, "change")
            if (!area.queuedEdit || !bound || area.host.pending || area.composing) return
            if (area.sentBinding !== null && bound.local === area.sentBinding) return
            var value = { text: input.text, revision: area.editRevision, reset: area.resetRevision }
            if (area.host.invoke("change", value)) { area.queuedEdit = false; area.sentBinding = bound.local || null }
        }
    }
    Connections {
        target: area.host
        function onModelChanged() { area.synchronize() }
    }
    onGivenChanged: synchronize()
    Component.onCompleted: { synchronize(); if (Props.textareaAutofocus(host.model)) Qt.callLater(function() { input.forceActiveFocus() }) }

    implicitWidth: host.space(240)
    readonly property int lineHeight: Math.round(area.textSize * 1.4)
    implicitHeight: labelHeight + area.rows * area.lineHeight + host.space(16)
    readonly property real labelHeight: areaLabel.visible ? areaLabel.implicitHeight + host.space(6) : 0

    Text {
        id: areaLabel
        width: parent.width
        text: Props.textareaLabel(area.host.model)
        visible: text !== ""
        wrapMode: Text.Wrap
        color: area.host.ink
        font.family: area.host.fontFamily
        font.pixelSize: area.textSize
    }

    Rectangle {
        anchors.fill: parent
        anchors.topMargin: area.labelHeight
        radius: host.radius
        color: area.host.controlFill(input.activeFocus, hover.containsMouse)

        HoverHandler { id: hover }

        TextEdit {
            id: input
            cursorVisible: activeFocus && area.host.theme.motion
            objectName: "editor"
            anchors.fill: parent
            anchors.leftMargin: area.host.space(10)
            anchors.rightMargin: area.host.space(10)
            anchors.topMargin: area.host.space(8)
            anchors.bottomMargin: area.host.space(8)
            wrapMode: TextEdit.Wrap
            clip: true
            color: area.host.ink
            font.family: area.host.fontFamily
            font.pixelSize: area.textSize
            selectByMouse: true
            activeFocusOnTab: true
            readOnly: area.controlled ? !area.enabled : !area.host.interactive

            // TextEdit has no textEdited on Qt 6.4; a guarded textChanged is
            // equivalent and programmatic writes are not edits.
            onTextChanged: if (!area.setting) area.edited()
            Accessible.role: Accessible.EditableText
            Accessible.name: areaLabel.text

            Text {
                anchors.fill: parent
                anchors.topMargin: area.host.space(8)
                verticalAlignment: Text.AlignTop
                text: Props.textareaPlaceholder(area.host.model)
                color: Qt.darker(area.host.ink, 1.4)
                font.family: area.host.fontFamily
                font.pixelSize: area.textSize
                wrapMode: Text.Wrap
                visible: input.text === ""
            }
        }
    }
}
