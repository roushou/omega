import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "../core"

ShellRoot {
    id: preview
    property var snapshot: ({})
    property int sequence: 0
    property bool capturing: false
    property bool captured: false
    property string failure: ""
    property int viewportWidth: snapshot.width || 600
    property int viewportHeight: snapshot.height || 560
    property bool light: snapshot.theme === "light"
    readonly property bool captureMode: !!snapshot.capturePath
    Requests { id: requests }
    Assets {
        id: previewAssets
        iconResolver: name => {
            if (preview.captureMode) {
                preview.failure = "Use fixed local fixture images for capture; theme icon providers require a native graphics backend."
                return ""
            }
            return Quickshell.iconPath(name, "application-x-executable")
        }
    }
    Theme {
        id: previewTheme
        foreground: preview.light ? "#18181b" : "#e4e4e7"
        background: preview.light ? "#fafafa" : "#18181b"
        muted: preview.light ? "#52525b" : "#a1a1aa"
        accent: preview.light ? "#1d4ed8" : "#93c5fd"
        font: ({family: "DejaVu Sans", body: 14, caption: 12, icon: 16, subtitle: 16, title: 18, heading: 22, display: 28})
        motion: !preview.captureMode
    }
    Socket {
        id: socket
        path: Quickshell.env("OMEGA_PREVIEW_HOST_SOCKET")
        connected: true
        parser: SplitParser {
            onRead: data => {
                try {
                    var next = JSON.parse(data)
                    if (next.version !== 1) { preview.failure = "Preview protocol mismatch"; return }
                    if (next.stale || next.epoch !== preview.snapshot.epoch)
                        requests.disconnected("Preview changed; reset pending interactions.")
                    preview.snapshot = next
                    if (next.answered) requests.finish(String(next.answered), {done: true, error: next.error ? {message: next.error} : null})
                    settling.restart()
                } catch (error) { preview.failure = String(error) }
            }
        }
        onConnectedChanged: if (!connected) { requests.disconnected("Preview session disconnected"); preview.failure = "Preview session disconnected" }
    }
    function send(command, key) {
        if (!socket.connected || preview.snapshot.stale) return false
        sequence += 1
        var id = String(sequence)
        if (key && !requests.begin(id, key, Date.now())) return false
        command.id = id
        socket.write(JSON.stringify(command) + "\n")
        socket.flush()
        return true
    }
    QtObject {
        id: previewSession
        property Navigation navigation: Navigation {}
        property var requests: requestsProxy
        function press(bound, value, key, event) {
            if (preview.captureMode) return false
            return preview.send({interact: {revision: preview.snapshot.view.revision, node: key, event: event,
                value: value === undefined ? null : preview.value(value)}}, key)
        }
    }
    Requests { id: requestsProxy }
    Connections {
        target: requests
        function onSettled(key, success) { requestsProxy.error = requests.error; requestsProxy.settled(key, success) }
        function onPendingChanged() { requestsProxy.pending = requests.pending }
    }
    function value(input) {
        if (input === null) return {}
        if (typeof input === "boolean") return {boolValue: input}
        if (typeof input === "number") return Number.isInteger(input) ? {intValue: String(input)} : {doubleValue: input}
        if (typeof input === "string") return {stringValue: input}
        if (Array.isArray(input)) return {list: {values: input.map(preview.value)}}
        var entries = {}
        for (var key in input) entries[key] = preview.value(input[key])
        return {map: {entries: entries}}
    }
    FloatingWindow {
        id: window
        title: "Omega preview — " + (preview.snapshot.selected || "loading")
        visible: true
        color: "#242428"
        implicitWidth: preview.viewportWidth + (preview.captureMode ? 0 : 300)
        implicitHeight: preview.viewportHeight + (preview.captureMode ? 0 : 88)
        minimumSize: Qt.size(320, 240)
        onClosed: Qt.quit()
        Row {
            id: toolbar
            visible: !preview.captureMode
            height: visible ? 44 : 0
            spacing: 8
            ComboBox { width: 180; model: preview.snapshot.cases || []; currentIndex: model.indexOf(preview.snapshot.selected); onActivated: preview.send({select: currentText}) }
            Button { text: "Reset"; enabled: !preview.snapshot.stale; onClicked: preview.send({reset: true}) }
            ComboBox { model: ["dark", "light"]; currentIndex: preview.light ? 1 : 0; onActivated: preview.light = currentIndex === 1 }
            SpinBox { from: 64; to: 4096; value: preview.viewportWidth; editable: true; onValueModified: preview.viewportWidth = value }
            SpinBox { from: 64; to: 4096; value: preview.viewportHeight; editable: true; onValueModified: preview.viewportHeight = value }
        }
        Viewport {
            id: canvas
            x: 0; y: toolbar.height
            width: preview.viewportWidth
            height: preview.viewportHeight
            theme: previewTheme
            assets: previewAssets
            session: previewSession
            view: preview.snapshot.view || null
            epoch: preview.snapshot.epoch || "0"
            enabled: !preview.snapshot.stale
            visible: !preview.snapshot.presentation || preview.snapshot.presentation === "PRESENTATION_STATE_VISIBLE"
        }
        ScrollView {
            visible: !preview.captureMode
            x: canvas.width + 12; y: toolbar.height
            width: 276; height: parent.height - toolbar.height - 44
            Column {
                width: 264; spacing: 10
                Label { text: "Isolated effects"; color: "white"; font.bold: true }
                Label { width: parent.width; text: "Effects wait here. Resolve now or leave pending to inspect loading. No live services are called."; wrapMode: Text.Wrap; color: "#b0b0b8" }
                Repeater {
                    model: preview.snapshot.effects || []
                    Column {
                        required property var modelData
                        Label { text: modelData.operation; color: "white" }
                        Row {
                            Button { text: "Succeed"; onClicked: preview.send({resolve: {effect: modelData.id, success: true}}) }
                            Button { text: "Refuse"; onClicked: preview.send({resolve: {effect: modelData.id, success: false}}) }
                        }
                    }
                }
                Label { text: "Events (values omitted)"; color: "white"; font.bold: true }
                Repeater { model: preview.snapshot.events || []; Label { required property string modelData; text: modelData; color: "#b0b0b8" } }
            }
        }
        Label {
            visible: !preview.captureMode
            anchors.bottom: parent.bottom; width: parent.width; height: 44
            text: preview.failure || preview.snapshot.error || requests.error || (preview.snapshot.presentation === "PRESENTATION_STATE_CLOSED" ? "Presentation closed · Reset to reopen" : preview.snapshot.presentation === "PRESENTATION_STATE_HIDDEN" ? "Presentation hidden · Reset to reopen" : "Synthetic fixtures · changes rebuild automatically")
            color: preview.snapshot.stale || preview.failure || preview.snapshot.error ? "#fda4af" : "#a1a1aa"
            wrapMode: Text.Wrap
        }
    }
    Timer {
        id: settling
        interval: 250
        onTriggered: {
            if (!preview.captureMode || preview.capturing || preview.captured || !preview.snapshot.view) return
            if (preview.snapshot.stale || preview.snapshot.error || preview.failure) { preview.send({captured: preview.failure || preview.snapshot.error || "stale preview"}); return }
            if (previewAssets.pendingImages.length) { settling.restart(); return }
            if (previewAssets.failedImages.length) { preview.send({captured: "asset failed to load"}); return }
            preview.capturing = true
            canvas.grabToImage(function(result) {
                var saved = result.saveToFile(preview.snapshot.capturePath)
                preview.captured = saved
                preview.send({captured: saved ? preview.snapshot.capturePath : "capture could not be saved"})
            }, Qt.size(preview.viewportWidth, preview.viewportHeight))
        }
    }
}
