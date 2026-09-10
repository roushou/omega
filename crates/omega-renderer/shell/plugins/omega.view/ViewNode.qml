import QtQuick
import qs.Commons
import "Props.js" as Props

// One node of a published view tree, and its children under it.
//
// A dispatcher: the node's `type` names a file under `nodes/`, which is
// loaded by url. Loading by url rather than by component is also what lets a
// stack contain a `ViewNode` again — QML refuses a component that names
// itself, "ViewNode is instantiated recursively", and a url is resolved when
// it is reached rather than when this file is compiled.
//
// A type this shell has no file for draws nothing rather than guessing, so a
// tree from a newer unit degrades to the parts this shell understands instead
// of failing whole.
//
// Every delegate is handed exactly one property: `host`, this object. It
// reads `host.model` and `host.ink` rather than taking copies, so a new tree
// reaches it through the same bindings instead of needing the delegate
// rebuilt — and it calls `host.invoke` to send an interaction back up, with
// the value the control carries when it has one.
Item {
    id: node

    // The node object, as it arrived in the JSON line.
    required property var model
    // What text is drawn in, unless a node says otherwise. The host passes
    // its own foreground down, so a tree is themed like everything beside it.
    property color foreground: Color.foreground

    // Which way the stack holding this node runs, passed down by that stack.
    // One node cares: a rule lies across its parent, so it is horizontal in a
    // column and vertical in a row. A node that is nobody's child — a
    // surface's root, a grid's cell — is told nothing and reads as a column,
    // which is the way a panel runs.
    property string axis: "column"

    // The colour this node draws in: what it asked for, or what it inherited.
    readonly property color ink: node.colorOf()

    // ------------------------------------------------------------- tokens
    //
    // Every dimension, weight and tint a delegate draws with comes from here,
    // and here reads the shell's `Style`. On `host` rather than in a file of
    // its own for two reasons: a delegate is handed exactly one property and
    // this keeps that true, and `colorOf` below was already doing this job
    // for colour alone.
    //
    // A number off the wire is an *intent*, not a pixel count. A unit saying
    // `gap(6)` means one comfortable gap; what that measures on a scaled
    // display under a large theme font is the shell's to decide, and no unit
    // can know it. `Style.space` is the exact migration Omarchy documents for
    // its own hardcoded values.
    function space(px) { return px > 0 ? Style.space(px) : 0 }

    readonly property string fontFamily: Style.font.family
    readonly property int fontSize: Style.font.body
    readonly property int captionSize: Style.font.caption
    readonly property int iconSize: Style.font.icon

    // What corners do, following the theme — which follows Hyprland's
    // `decoration:rounding`. A square theme gets square controls, the way
    // `ToggleSwitch` decides it.
    // A role on the type scale, as a unit names it. An unknown role — a
    // tree from a newer plugin — falls back rather than laying out a NaN,
    // which is the same bargain `delegateFor` makes for an unknown kind.
    function typeSize(role, fallback) {
        switch (role) {
            case "caption":  return Style.font.caption
            case "body":     return Style.font.body
            case "subtitle": return Style.font.subtitle
            case "title":    return Style.font.title
            case "heading":  return Style.font.heading
            case "display":  return Style.font.display
            default:         return fallback
        }
    }

    readonly property int radius: Style.cornerRadius
    readonly property bool rounded: Style.cornerRadius > 0
    function pill(size) { return node.rounded ? size / 2 : 0 }

    // Control chrome, at the theme's own state alphas.
    readonly property color idleFill: Style.normalFillFor(node.foreground, Color.accent, Color.urgent)
    readonly property color hoverFill: Style.hoverFillFor(node.foreground, Color.accent, Color.urgent)
    readonly property color chosenFill: Style.selectedFillFor(node.foreground, Color.accent, Color.urgent)
    readonly property color hoverInk: Style.hoverStateColor(node.foreground, Color.accent, Color.urgent)

    // A track is chrome the eye has to find — a slider's groove, the unfilled
    // part of a bar — so it takes the selected fill. The idle one is 4%, and
    // is meant to sit behind a border rather than stand on its own.
    readonly property color trackFill: node.chosenFill

    // What an input sits in, through the kit's own function so a focused or
    // hovered field looks like every other focused or hovered field.
    function controlFill(focused, hot) {
        return Style.controlFill(focused, hot, node.foreground, Color.accent)
    }

    // The tint `PanelSeparator` draws with. A literal rather than a token
    // because the shell's own separator carries it as one, and a rule beside
    // Omarchy's rules has to match them rather than a scale.
    readonly property color rule:
        Qt.rgba(node.foreground.r, node.foreground.g, node.foreground.b, 0.12)

    // Whether this node may be used. Resolved here rather than in each
    // delegate so a new one gets it by reading `host.interactive`, and so the
    // two reasons a node is unusable are drawn the same way.
    readonly property bool disabled: Props.disabled(node.model)
    readonly property bool busy: Props.busy(node.model)
    readonly property bool interactive: !node.disabled && !node.busy

    signal invoke(var bound, var value)

    // What a node asked to be, or what it draws. A panel that has to line
    // two columns up says so; everything else is its own size.
    readonly property int fixedWidth: node.space(Props.width(node.model))
    readonly property int fixedHeight: node.space(Props.height(node.model))

    // Room around what this node draws. Given here rather than by each
    // delegate because this is the item that owns the slot: the delegate is
    // centred in it, so a larger slot is padding on every side.
    readonly property int padding: node.space(Props.pad(node.model))

    implicitWidth: node.fixedWidth > 0
        ? node.fixedWidth
        : content.implicitWidth + node.padding * 2
    implicitHeight: node.fixedHeight > 0
        ? node.fixedHeight
        : content.implicitHeight + node.padding * 2

    // One reason to be unusable looks like the other. A shell with a spinner
    // would tell them apart here, and nothing above would change.
    opacity: node.interactive ? 1.0 : 0.5

    // Which file draws this node. Empty for a type this shell does not know.
    function delegateFor(type) {
        switch (type) {
            case "text": return "nodes/Text.qml"
            case "icon": return "nodes/Icon.qml"
            case "progress": return "nodes/Progress.qml"
            case "button": return "nodes/Button.qml"
            case "slider": return "nodes/Slider.qml"
            case "toggle": return "nodes/Toggle.qml"
            case "field": return "nodes/Field.qml"
            case "list": return "nodes/List.qml"
            case "stack": return "nodes/Stack.qml"
            case "separator": return "nodes/Separator.qml"
            case "spacer": return "nodes/Spacer.qml"
            case "header": return "nodes/Header.qml"
            case "graph": return "nodes/Graph.qml"
            case "group": return "nodes/Group.qml"
            case "grid": return "nodes/Grid.qml"
            case "image": return "nodes/Image.qml"
            default: return ""
        }
    }

    // A colour a node asked for, or the one it inherited. A theme name is
    // resolved here; anything else is taken literally, so `#ff8800` works.
    function colorOf() {
        var named = Props.color(node.model)
        var base = named === "" ? node.foreground : node.themed(named)
        return Props.dim(node.model)
            ? Qt.rgba(base.r, base.g, base.b, 0.6)
            : base
    }

    // Roles come from the shell's own palette, so a unit inherits the theme
    // the user chose without knowing one exists. Hardcoded hex here meant
    // every widget drew One Dark whatever the desktop was set to.
    //
    // A role this shell does not know falls back to what the node inherited
    // rather than being taken literally. Taking it literally is how a unit
    // could name `#ff8800` and draw a colour no theme chose — the SDK closed
    // that off, and this is the other half of closing it.
    function themed(name) {
        switch (name) {
            case "urgent": return Color.urgent
            case "accent": return Color.accent
            case "muted": return Color.muted
            case "foreground": return Color.foreground
            case "background": return Color.background
            default: return node.foreground
        }
    }

    Loader {
        id: content
        anchors.centerIn: parent

        // A node draws in the room it was given, which is its own size until
        // a layout gives it more: a bar told to span a panel is handed the
        // panel's width here, and a rule its length. Never less than what it
        // draws — a node squeezed below its own size is a clipped one, and
        // clipping a reading is worse than overflowing it.
        width: Math.max(content.implicitWidth, node.width - node.padding * 2)
        height: Math.max(content.implicitHeight, node.height - node.padding * 2)

        // Set from the type alone: the delegate reads everything else off
        // `host`, so a changed tree flows through bindings rather than
        // rebuilding the item and losing whatever state it held.
        readonly property string delegateUrl:
            node.model ? node.delegateFor(node.model.type) : ""

        onDelegateUrlChanged: content.load()
        Component.onCompleted: content.load()

        function load() {
            if (delegateUrl === "") {
                setSource("")
                return
            }
            setSource(delegateUrl, { "host": node })
        }
    }
}
