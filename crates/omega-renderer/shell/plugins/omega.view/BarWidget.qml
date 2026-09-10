import QtQuick
import qs.Ui
import qs.Commons
import "Props.js" as Props

// Draws the view trees an Omega unit publishes: one in the bar's slot, and
// one in a popout anchored to it.
//
// The socket half is `Connection`: which line is a view, which view is this
// one, and how a press travels back. What is left here is chrome. Two
// surfaces means two `Connection`s and one shared everything-else, which is
// the whole reason the socket was pulled out of here.
//
// Whether the popout is open is the shell's: opening a panel is what the user
// is doing, not a fact about the machine, so no unit is told and none has to
// be asked. What is *in* it is the unit's, and a unit that renders nothing
// for that surface has a panel with nothing in it.
BarWidget {
  id: root
  moduleName: "omega.view"

  readonly property string unit: setting("unit", "")
  readonly property string surface: setting("surface", "")
  readonly property string module: setting("module", "")
  readonly property string panel: setting("panel", "")
  readonly property string socketPath: setting("socket", "")

  readonly property bool hasPanel: root.panel !== ""
  property bool opened: false

  // `open`, `close` and `opened` together are what `Bar.findPanelWidget`
  // looks for. A widget missing any of the three is skipped, so a panel that
  // opens on a press would still not answer `omarchy shell toggle`.
  function open() { root.opened = true }
  function close() { root.opened = false }

  Connection {
    id: link
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
    tooltipText: link.tree && link.tree.root ? Props.tooltip(link.tree.root) : ""
    hasVisualContent: link.tree !== null
    // A button with no label is a button with no width, and the bar lays out
    // by implicit size — so the slot has to follow the tree instead.
    fixedWidth: view.implicitWidth > 0
      ? view.implicitWidth + button.scaledHorizontalMargin * 2
      : 0

    ViewNode {
      id: view
      anchors.centerIn: parent
      model: link.tree
      foreground: button.foreground
      visible: link.tree !== null
      onInvoke: (bound, value) => link.press(bound, value)
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
      // As wide as what it draws, which is how the height already worked.
      // A fixed 320 made every panel that width whatever was in it, so a
      // narrow one sat in a wide card and read as enormous side padding —
      // the card's own `padding` is the kit's `popupPadding` and was never
      // the problem. The floor keeps a panel with one line in it from
      // arriving as a sliver.
      contentWidth: fittedContentWidth(
        Math.max(Style.space(220), panelView.implicitWidth + padding * 2))
      contentHeight: fittedContentHeight(Math.max(Style.space(40), panelView.implicitHeight))

      ViewNode {
        id: panelView
        // Across the card, not centred in it: under the floor above, a tree
        // narrower than its card is a bar stopping short of what it measures.
        anchors.left: parent ? parent.left : undefined
        anchors.right: parent ? parent.right : undefined
        model: panelLink.tree
        foreground: Color.popups.text
        visible: panelLink.tree !== null
        onInvoke: (bound, value) => panelLink.press(bound, value)
      }
    }
  }
}
