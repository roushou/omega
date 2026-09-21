import QtQuick
import "../Props.js" as Props

// Inline confirm card. Modal stacking above an instance belongs to presentation;
// this node draws the card, its actions, and outside/Escape dismissal.
Rectangle {
    id: dialog
    required property var host

    readonly property var confirmBound: Props.bind(host.model, "confirm")
    readonly property var cancelBound: Props.bind(host.model, "cancel")
    readonly property var dismissBound: Props.bind(host.model, "dismiss")
    readonly property bool pressable: dialog.host.interactive

    implicitWidth: Math.max(host.space(240), host.fixedWidth)
    implicitHeight: host.space(24) + title.implicitHeight + bodySpace + buttons.implicitHeight

    readonly property real bodySpace: body.visible ? body.implicitHeight + host.space(10) : 0

    radius: host.radius
    color: host.theme.background
    border.width: host.space(1)
    border.color: host.rule

    // Escape dismisses only when a handler is bound.
    Keys.onEscapePressed: {
        if (dialog.dismissBound !== null && dialog.pressable) dialog.host.invoke("dismiss", undefined)
    }
    focus: true

    Column {
        anchors.fill: parent
        anchors.margins: host.space(12)
        spacing: host.space(8)

        Text {
            id: title
            width: parent.width
            text: Props.dialogTitle(host.model)
            color: host.ink
            font.family: host.fontFamily
            font.pixelSize: host.typeSize("title", host.fontSize)
            font.bold: true
            wrapMode: Text.Wrap
        }

        Text {
            id: body
            width: parent.width
            visible: text !== ""
            text: Props.dialogBody(host.model)
            color: host.ink
            opacity: 0.8
            font.family: host.fontFamily
            font.pixelSize: host.fontSize
            wrapMode: Text.Wrap
        }

        Row {
            id: buttons
            spacing: host.space(8)

            Rectangle {
                id: cancelButton
                visible: cancelBound !== null
                height: Math.max(host.space(36), cancelLabel.implicitHeight + host.space(16))
                width: Math.max(host.space(96), cancelLabel.implicitWidth + host.space(24))
                radius: host.radius
                color: cancelHover.containsMouse ? host.hoverFill : host.idleFill
                activeFocusOnTab: true
                enabled: cancelBound !== null && dialog.pressable
                border.width: cancelButton.activeFocus ? host.space(1) : 0
                border.color: host.ink

                function activate() {
                    if (cancelBound !== null && dialog.pressable) dialog.host.invoke("cancel", undefined)
                }
                Keys.onReturnPressed: cancelButton.activate()
                Keys.onEnterPressed: cancelButton.activate()
                Keys.onSpacePressed: cancelButton.activate()

                Text {
                    id: cancelLabel
                    anchors.centerIn: parent
                    text: Props.dialogCancel(host.model) || "Cancel"
                    color: host.ink
                    font.family: host.fontFamily
                    font.pixelSize: host.fontSize
                }

                MouseArea {
                    id: cancelHover
                    anchors.fill: parent
                    hoverEnabled: true
                    enabled: cancelBound !== null && dialog.pressable
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        cancelButton.forceActiveFocus()
                        cancelButton.activate()
                    }
                }
            }

            Rectangle {
                id: confirmButton
                visible: confirmBound !== null
                height: Math.max(host.space(36), confirmLabel.implicitHeight + host.space(16))
                width: Math.max(host.space(96), confirmLabel.implicitWidth + host.space(24))
                radius: host.radius
                color: confirmHover.containsMouse ? host.hoverFill : host.chosenFill
                activeFocusOnTab: true
                enabled: confirmBound !== null && dialog.pressable
                border.width: confirmButton.activeFocus ? host.space(1) : 0
                border.color: host.ink

                function activate() {
                    if (confirmBound !== null && dialog.pressable) dialog.host.invoke("confirm", undefined)
                }
                Keys.onReturnPressed: confirmButton.activate()
                Keys.onEnterPressed: confirmButton.activate()
                Keys.onSpacePressed: confirmButton.activate()

                Text {
                    id: confirmLabel
                    anchors.centerIn: parent
                    text: Props.dialogConfirm(host.model) || "Confirm"
                    color: host.ink
                    font.family: host.fontFamily
                    font.pixelSize: host.fontSize
                }

                MouseArea {
                    id: confirmHover
                    anchors.fill: parent
                    hoverEnabled: true
                    enabled: confirmBound !== null && dialog.pressable
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        confirmButton.forceActiveFocus()
                        confirmButton.activate()
                    }
                }
            }
        }
    }
}
