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

    // The colour this node draws in: what it asked for, or what it inherited.
    readonly property color ink: node.colorOf()

    signal invoke(var bound, var value)

    implicitWidth: content.implicitWidth
    implicitHeight: content.implicitHeight

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
            default: return ""
        }
    }

    // A colour a node asked for, or the one it inherited. A theme name is
    // resolved here; anything else is taken literally, so `#ff8800` works.
    function colorOf() {
        var named = Props.text(node.model, "color", "")
        var base = named === "" ? node.foreground : node.themed(named)
        return Props.flag(node.model, "dim", false)
            ? Qt.rgba(base.r, base.g, base.b, 0.6)
            : base
    }

    // Theme names come from the shell's own palette, so a unit inherits the
    // theme the user chose without knowing one exists. Hardcoded hex here
    // meant every widget drew One Dark whatever the desktop was set to.
    function themed(name) {
        switch (name) {
            case "urgent": return Color.urgent
            case "accent": return Color.accent
            case "muted": return Color.muted
            case "foreground": return Color.foreground
            case "background": return Color.background
            default: return name
        }
    }

    Loader {
        id: content
        anchors.centerIn: parent

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
