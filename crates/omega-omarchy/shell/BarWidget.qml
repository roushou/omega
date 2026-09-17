import "core"
import QtQuick
import Quickshell
import qs.Ui
import qs.Commons
import "core/Props.js" as Props

// The adapter owns native bar/popup chrome; instance intent and interactions
// pass through scoped renderer connections.
BarWidget {
  id: root
  OmarchyTheme { id: desktopTheme }
  Assets { id: desktopAssets; iconResolver: name => Quickshell.iconPath(name, "application-x-executable") }
  InstanceSession { id: indicatorSession; connection: link; snapshot: link ? link.snapshot : null }
  InstanceSession { id: panelSession; connection: panelLink; snapshot: panelLink ? panelLink.snapshot : null }
  moduleName: "omega.view"

  readonly property string plugin: setting("plugin", "")
  readonly property string surface: setting("surface", "")
  readonly property string module: setting("module", "")
  readonly property string panel: setting("panel", "")
  readonly property string socketPath: setting("socket", "")

  readonly property bool hasPanel: root.panel !== ""
  visible: !link || link.instance === null || link.presented
  onVisibleChanged: reportIndicator()
  function reportIndicator() {
    if (!link || !link.instance || !link.snapshot) return
    var shown = root.visible && link.presented
    var state = shown ? "PRESENTATION_STATE_VISIBLE" : "PRESENTATION_STATE_HIDDEN"
    var observed = link.snapshot.observed
    if (observed !== state && observed !== (shown ? 2 : 1)) link.report(link.instance, state)
  }
  readonly property var panelState: panelLink ? panelLink.panelSession : null
  property var registeredPanel: null
  function registerPanel() {
    if (registeredPanel === panelState) return
    if (registeredPanel) registeredPanel.releaseHost(root)
    registeredPanel = panelState
    if (registeredPanel) registeredPanel.registerHost(root)
  }
  onPanelStateChanged: registerPanel()
  Component.onCompleted: registerPanel()
  Component.onDestruction: if (registeredPanel) registeredPanel.releaseHost(root)
  readonly property bool opened: panelState !== null && panelState.owner === root && panelState.opened

  // Bar.findPanelWidget requires open, close, and opened for shell toggle support.
  function open() { if (panelState) panelState.setOpen(true, root) }
  function close() { if (panelState && panelState.owner === root) panelState.setOpen(false) }

  readonly property var link: indicatorConnection.connection
  readonly property var panelLink: panelConnection.connection
  Connections {
    target: root.link
    function onViewUpdated() { root.reportIndicator() }
  }

  PlacementConnection {
    id: indicatorConnection
    plugin: root.plugin
    surface: root.surface
    module: root.module
    // An empty socket path selects the shared transport default.
    socketPath: root.socketPath
  }

  // The popout's own view. A second surface of the same plugin and the same
  // module instance — the document placed one thing, which draws in two.
  PlacementConnection {
    id: panelConnection
    plugin: root.plugin
    surface: root.panel
    module: root.module
    socketPath: root.socketPath
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    // Omarchy's drag overlay forwards registered clicks before child controls.
    // Only popup triggers consume that forwarding; other trees receive the click.
    pressable: root.hasPanel
    // The tree draws itself; the button is the bar's chrome around it.
    labelVisible: false
    // Expose the root node tooltip through the host bar tooltip window.
    tooltipText: link ? link.requests.error || link.status || (link.tree ? Props.tooltip(link.tree) : "") : ""
    hasVisualContent: (link !== null && link.tree !== null) || fallback.visible
    // A button with no label is a button with no width, and the bar lays out
    // by implicit size — so the slot has to follow the tree instead.
    fixedWidth: view.implicitWidth > 0 || fallback.visible
      ? Math.max(view.implicitWidth, fallback.visible ? fallback.implicitWidth : 0) + button.scaledHorizontalMargin * 2
      : 0

    Text {
      id: fallback
      anchors.centerIn: parent
      visible: link !== null && link.tree === null && (link.status !== "" || link.requests.error !== "")
      text: link && link.requests.error !== "" ? "!" : "…"
      color: link && link.requests.error !== "" ? Color.urgent : button.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.body
    }

    ViewNode {
        focus: true
      assets: desktopAssets
      id: view
      anchors.centerIn: parent
      theme: desktopTheme
      model: link ? link.tree : null
      foreground: button.foreground
      visible: link !== null && link.tree !== null
      session: indicatorSession
    }

    // Child controls must receive clicks before the popup trigger.
    // Parent `view` after WidgetButton's MouseArea; a sibling below it cannot receive clicks.
    onPressed: function (mouseButton) {
      if (root.panelState && mouseButton === Qt.LeftButton) root.panelState.toggle(root)
    }
  }

  // KeyboardPanel owns popup anchoring, focus, and dismissal.
  Loader {
    active: root.hasPanel
    sourceComponent: KeyboardPanel {
      anchorItem: button
      owner: root
      bar: root.bar
      open: root.opened
      focusTarget: panelView
      contentWidth: fittedContentWidth(
        Math.max(Style.space(320), panelView.implicitWidth + padding * 2), Style.space(480))
      contentHeight: fittedContentHeight(Math.max(Style.space(40), panelContents.implicitHeight))

      Flickable {
        id: viewport
        Keys.onEscapePressed: root.close()
        contentWidth: width
        anchors.fill: parent
        contentHeight: panelContents.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        Column {
          id: panelContents
          width: viewport.width
          spacing: Style.space(8)
          Text {
            width: parent.width
            visible: text !== ""
            text: panelLink ? panelLink.requests.error || panelLink.status : ""
            color: panelLink && panelLink.requests.error !== "" ? Color.urgent : Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            wrapMode: Text.Wrap
          }
          ViewNode {
              focus: true
      assets: desktopAssets
            id: panelView
            width: parent.width
            theme: desktopTheme
            model: panelLink ? panelLink.tree : null
            foreground: Color.popups.text
            visible: panelLink !== null && panelLink.tree !== null
            session: panelSession
          }
        }
      }
    }
  }
}
