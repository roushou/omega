import QtQuick
import Quickshell
import Quickshell.Wayland
import "core"

ShellRoot {
    id: desktop
    RendererConnection {
        id: link
        plugin: Quickshell.env("OMEGA_RENDERER_PLUGIN")
        socketPath: Quickshell.env("OMEGA_RENDERER_SOCKET")
        standalone: true
    }
    DesktopTheme { id: desktopTheme }
    Assets { id: desktopAssets; iconResolver: name => Quickshell.iconPath(name, "application-x-executable") }
    Variants {
        model: Object.keys(link.instances)
        delegate: Scope {
            id: instance
            required property var modelData
            readonly property var snapshot: link.instances[modelData] || null
            readonly property var identity: snapshot ? snapshot.instance : null
            readonly property var presentation: snapshot ? snapshot.presentation || {} : ({})
            readonly property bool shown: snapshot !== null && (snapshot.requested === 2 || snapshot.requested === "PRESENTATION_STATE_VISIBLE")
            readonly property var tree: snapshot && snapshot.view ? snapshot.view.root || null : null
            property InstanceSession session: InstanceSession { connection: link; snapshot: instance.snapshot }
            function report(visible) {
                link.report(instance.identity, visible ? "PRESENTATION_STATE_VISIBLE" : (instance.snapshot && (instance.snapshot.requested === 3 || instance.snapshot.requested === "PRESENTATION_STATE_CLOSED") ? "PRESENTATION_STATE_CLOSED" : "PRESENTATION_STATE_HIDDEN"))
            }
            FloatingWindow {
                id: window
                visible: !!instance.presentation.window && instance.shown
                title: instance.presentation.window ? instance.presentation.window.title : "Omega"
                implicitWidth: instance.presentation.window ? instance.presentation.window.width : 480
                implicitHeight: instance.presentation.window ? instance.presentation.window.height : 320
                minimumSize: Qt.size(instance.presentation.window ? instance.presentation.window.minWidth : 1,
                    instance.presentation.window ? instance.presentation.window.minHeight : 1)
                color: desktopTheme.background
                onVisibleChanged: if (instance.presentation.window) instance.report(visible)
                onClosed: link.report(instance.identity, "PRESENTATION_STATE_CLOSED")
                Text { anchors.bottom: parent.bottom; anchors.left: parent.left; anchors.right: parent.right; anchors.margins: 8; text: instance.session.requests.error; color: desktopTheme.urgent; wrapMode: Text.Wrap; z: 1 }
                ViewNode { focus: true; assets: desktopAssets; anchors.fill: parent; anchors.margins: desktopTheme.surfacePadding; model: instance.tree; theme: desktopTheme; session: instance.session; }
            }
            PanelWindow {
                id: overlay
                readonly property var spec: instance.presentation.overlay || null
                visible: spec !== null && instance.shown && (!spec.output || targetScreen !== null)
                implicitWidth: spec ? spec.width : 480
                implicitHeight: spec ? spec.height : 320
                exclusionMode: ExclusionMode.Ignore
                readonly property bool modal: spec !== null && !!spec.dismissOnOutside
                anchors { top: modal; bottom: modal; left: modal; right: modal }
                color: "transparent"
                WlrLayershell.namespace: "omega-" + link.plugin
                WlrLayershell.layer: WlrLayer.Overlay
                WlrLayershell.keyboardFocus: !spec || spec.keyboard === "KEYBOARD_POLICY_NONE" ? WlrKeyboardFocus.None
                    : spec.keyboard === "KEYBOARD_POLICY_EXCLUSIVE" ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.OnDemand
                screen: targetScreen
                readonly property var targetScreen: {
                    if (!overlay.spec || !overlay.spec.output) return null
                    var screens = Quickshell.screens
                    for (var i = 0; i < screens.length; i++) if (screens[i].name === overlay.spec.output) return screens[i]
                    return null
                }
                onVisibleChanged: if (overlay.spec) instance.report(visible)
                MouseArea {
                    anchors.fill: parent
                    enabled: overlay.modal
                    onClicked: link.report(instance.identity, "PRESENTATION_STATE_CLOSED")
                }
                Rectangle {
                    anchors.centerIn: parent
                    width: overlay.modal ? Math.min(overlay.spec.width, parent.width - 32) : parent.width
                    height: overlay.modal ? Math.min(overlay.spec.height, parent.height - 32) : parent.height
                    color: desktopTheme.background
                    radius: desktopTheme.cornerRadius
                    border.width: desktopTheme.surfaceBorderWidth
                    border.color: desktopTheme.surfaceBorderColor
                    focus: true
                    // Consume clicks in padding without dismissing the content.
                    MouseArea { anchors.fill: parent }
                    Text { anchors.bottom: parent.bottom; anchors.left: parent.left; anchors.right: parent.right; anchors.margins: 8; text: instance.session.requests.error; color: desktopTheme.urgent; wrapMode: Text.Wrap; z: 1 }
                    Keys.onEscapePressed: link.report(instance.identity, "PRESENTATION_STATE_CLOSED")
                    ViewNode { focus: true; assets: desktopAssets; anchors.fill: parent; anchors.margins: desktopTheme.surfacePadding; model: instance.tree; theme: desktopTheme; session: instance.session; }
                }
            }
        }
    }
}
