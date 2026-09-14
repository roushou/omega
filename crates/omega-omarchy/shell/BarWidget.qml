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
  InstanceSession { id: indicatorSession; connection: link; snapshot: link.snapshot }
  InstanceSession { id: panelSession; connection: panelLink; snapshot: panelLink.snapshot }
  moduleName: "omega.view"

  readonly property string unit: setting("unit", "")
  readonly property string surface: setting("surface", "")
  readonly property string module: setting("module", "")
  readonly property string panel: setting("panel", "")
  readonly property string socketPath: setting("socket", "")

  readonly property bool hasPanel: root.panel !== ""
  visible: link.instance === null || link.presented
  onVisibleChanged: reportIndicator()
  function reportIndicator() {
    if (!link.instance || !link.snapshot) return
    var shown = root.visible && link.presented
    var state = shown ? "PRESENTATION_STATE_VISIBLE" : "PRESENTATION_STATE_HIDDEN"
    var observed = link.snapshot.observed
    if (observed !== state && observed !== (shown ? 2 : 1)) link.report(link.instance, state)
  }
  property bool opened: false
  onOpenedChanged: {
    panelLink.change(root.opened ? "PRESENTATION_ACTION_PRESENT" : "PRESENTATION_ACTION_HIDE")
    panelLink.report(panelLink.instance, root.opened ? "PRESENTATION_STATE_VISIBLE" : "PRESENTATION_STATE_HIDDEN")
  }

  // `open`, `close` and `opened` together are what `Bar.findPanelWidget`
  // looks for. A widget missing any of the three is skipped, so a panel that
  // opens on a press would still not answer `omarchy shell toggle`.
  function open() { root.opened = true }
  function close() { root.opened = false }

  Connection {
    id: link
    onViewUpdated: root.reportIndicator()
    unit: root.unit
    surface: root.surface
    module: root.module
    // Empty means the default, which `Connection` already knows: naming the
    // path here too would be a second place for it to be wrong.
    socketPath: root.socketPath !== "" ? root.socketPath : link.defaultSocketPath
  }

  // The popout's own view. A second surface of the same unit and the same
  // module instance — the document placed one thing, which draws in two.
  Connection {
    id: panelLink
    onViewUpdated: root.opened = panelLink.presented
    unit: root.unit
    surface: root.panel
    module: root.module
    socketPath: link.socketPath
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    // The tree draws itself; the button is the bar's chrome around it.
    labelVisible: false
    // What the slot's own root node asked to say on hover. The bar owns the
    // tooltip window, so this is the one node in a tree that can have one —
    // a tooltip deeper in would need a host that follows the pointer.
    tooltipText: link.requests.error || link.status || (link.tree ? Props.tooltip(link.tree) : "")
    hasVisualContent: link.tree !== null || fallback.visible
    // A button with no label is a button with no width, and the bar lays out
    // by implicit size — so the slot has to follow the tree instead.
    fixedWidth: view.implicitWidth > 0 || fallback.visible
      ? Math.max(view.implicitWidth, fallback.visible ? fallback.implicitWidth : 0) + button.scaledHorizontalMargin * 2
      : 0

    Text {
      id: fallback
      anchors.centerIn: parent
      visible: link.tree === null && (link.status !== "" || link.requests.error !== "")
      text: link.requests.error !== "" ? "!" : "…"
      color: link.requests.error !== "" ? Color.urgent : button.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.body
    }

    ViewNode {
      assets: desktopAssets
      id: view
      anchors.centerIn: parent
      theme: desktopTheme
      model: link.tree
      foreground: button.foreground
      visible: link.tree !== null
      session: indicatorSession
    }

    // Pressing the slot opens the popout — but only where the tree itself did
    // not want the press. A widget that drew a button has said what a press
    // means, and stealing it to open a panel would make the button dead.
    //
    // Ordering is what arranges that, rather than a second mouse area: `view`
    // is parented to the button after the button's own, so a node that
    // handles a click gets it and this never runs. Putting one *underneath*
    // cannot work — `WidgetButton` fills itself with a MouseArea that accepts
    // every button, and nothing below it is ever reached.
    onPressed: function (mouseButton) {
      if (root.hasPanel && mouseButton === Qt.LeftButton) root.opened = !root.opened
    }
  }

  // The popout, anchored to the slot. `KeyboardPanel` owns the layer-shell
  // window, focus on open, outside-click dismissal, and positioning against
  // the bar — all of which a panel needs and none of which is Omega's to
  // reinvent.
  Loader {
    active: root.hasPanel
    sourceComponent: KeyboardPanel {
      anchorItem: button
      owner: root
      bar: root.bar
      open: root.opened
      focusTarget: viewport
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
            text: panelLink.requests.error || panelLink.status
            color: panelLink.requests.error !== "" ? Color.urgent : Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            wrapMode: Text.Wrap
          }
          ViewNode {
      assets: desktopAssets
            id: panelView
            width: parent.width
            theme: desktopTheme
            model: panelLink.tree
            foreground: Color.popups.text
            visible: panelLink.tree !== null
            session: panelSession
          }
        }
      }
    }
  }
}
