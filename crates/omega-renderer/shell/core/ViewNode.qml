import QtQuick
import QtQuick.Window
import "Keyboard.js" as Keyboard
import "Props.js" as Props

FocusScope {
    id: node

    required property var model
    property Theme theme: Theme {}
    property Assets assets: Assets {}
    property color foreground: node.theme.foreground

    property string axis: "column"

    readonly property color ink: node.colorOf()

    function space(px) { return px > 0 ? node.theme.space(px) : 0 }

    readonly property string fontFamily: node.theme.font.family
    readonly property int fontSize: node.theme.font.body
    readonly property int captionSize: node.theme.font.caption
    readonly property int iconSize: node.theme.font.icon

    function typeSize(role, fallback) {
        switch (role) {
            case "caption":  return node.theme.font.caption
            case "body":     return node.theme.font.body
            case "subtitle": return node.theme.font.subtitle
            case "title":    return node.theme.font.title
            case "heading":  return node.theme.font.heading
            case "display":  return node.theme.font.display
            default:         return fallback
        }
    }

    readonly property int radius: node.theme.cornerRadius
    readonly property bool rounded: node.theme.cornerRadius > 0
    function pill(size) { return node.rounded ? size / 2 : 0 }

    readonly property color idleFill: node.theme.normalFillFor(node.foreground, node.theme.accent, node.theme.urgent)
    readonly property color hoverFill: node.theme.hoverFillFor(node.foreground, node.theme.accent, node.theme.urgent)
    readonly property color chosenFill: node.theme.selectedFillFor(node.foreground, node.theme.accent, node.theme.urgent)
    readonly property color hoverInk: node.theme.hoverStateColor(node.foreground, node.theme.accent, node.theme.urgent)

    readonly property color trackFill: node.chosenFill

    function controlFill(focused, hot) {
        return node.theme.controlFill(focused, hot, node.foreground, node.theme.accent)
    }

    readonly property color rule:
        Qt.rgba(node.foreground.r, node.foreground.g, node.foreground.b, 0.12)

    readonly property bool disabled: Props.disabled(node.model)
    readonly property bool busy: Props.busy(node.model)
    enabled: !node.disabled && !node.busy
    readonly property bool interactive: node.enabled && !node.pending

    property var session: null
    property var form: null
    readonly property bool pending: session !== null && (session.busy ? session.busy(model ? model.key : "") : session.requests.busy(model ? model.key : ""))
    function invoke(event, value) {
        if (!node.interactive || !node.session) return false
        var bound = Props.bind(node.model, event)
        if (!bound) return false
        return node.session.press(bound, value, node.model.key, event)
    }

    Keys.priority: Keys.AfterItem
    Keys.onPressed: event => node.shortcut(event, false)
    Keys.onReleased: event => node.shortcut(event, true)
    function shortcut(event, release) {
        if (!node.enabled || !node.visible || !node.model) return
        if (Keyboard.composing(node.Window.window ? node.Window.window.activeFocusItem : null)) {
            event.accepted = true
            return
        }
        var binding = Keyboard.resolve(node.model.shortcuts || [], event, release)
        if (!binding) return
        // A selected binding owns the event even if admission is refused or pending.
        event.accepted = true
        node.invoke(binding.event, undefined)
    }

    readonly property int fixedWidth: node.space(Props.width(node.model))
    readonly property int fixedHeight: node.space(Props.height(node.model))

    readonly property int padding: node.space(Props.pad(node.model))

    implicitWidth: node.fixedWidth > 0
        ? node.fixedWidth
        : content.implicitWidth + node.padding * 2
    implicitHeight: node.fixedHeight > 0
        ? node.fixedHeight
        : content.implicitHeight + node.padding * 2

    // Controlled editors keep accepting local drafts while a change is in flight.
    readonly property bool localEditing: node.model && node.model.type === "field" && Props.fieldControlled(node.model)
    opacity: node.enabled && (node.localEditing || !node.pending) ? 1.0 : 0.5

    function delegateFor(type) {
        switch (type) {
            case "text": return "nodes/Text.qml"
            case "icon": return "nodes/Icon.qml"
            case "progress": return "nodes/Progress.qml"
            case "button": return "nodes/Button.qml"
            case "slider": return "nodes/Slider.qml"
            case "toggle": return "nodes/Toggle.qml"
            case "checkbox": return "nodes/Checkbox.qml"
            case "dropdown": return "nodes/Dropdown.qml"
            case "disclosure": return "nodes/Disclosure.qml"
            case "dialog": return "nodes/Dialog.qml"
            case "form": return "nodes/Form.qml"
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
            case "badge": return "nodes/Badge.qml"
            case "keycap": return "nodes/Keycap.qml"
            case "status": return "nodes/Status.qml"
            case "scroll": return "nodes/Scroll.qml"
            default: return ""
        }
    }

    function colorOf() {
        var named = Props.color(node.model)
        var base = named === "" ? node.foreground : node.themed(named)
        switch (Props.tone(node.model)) {
            case "warning":
            case "error": base = node.theme.urgent; break
            case "success": base = node.theme.accent; break
        }
        var emphasis = Props.emphasis(node.model)
        if (emphasis === "primary" && (Props.tone(node.model) === "" || Props.tone(node.model) === "neutral")) base = node.theme.accent
        var alpha = emphasis === "muted" || Props.dim(node.model) ? 0.6 : 1.0
        return Qt.rgba(base.r, base.g, base.b, base.a * alpha)
    }

    function themed(name) {
        switch (name) {
            case "urgent": return node.theme.urgent
            case "accent": return node.theme.accent
            case "muted": return node.theme.muted
            case "foreground": return node.theme.foreground
            case "background": return node.theme.background
            default: return node.foreground
        }
    }

    Loader {
        id: content
        anchors.centerIn: parent

        width: Math.max(0, node.width - node.padding * 2)
        height: Math.max(content.implicitHeight, node.height - node.padding * 2)

        readonly property string delegateUrl:
            node.model ? node.delegateFor(node.model.type) : ""

        onDelegateUrlChanged: content.load()
        Component.onCompleted: content.load()

        function load() {
            // Both the URL binding and completion can request the initial load.
            var resolved = delegateUrl === "" ? "" : Qt.resolvedUrl(delegateUrl).toString()
            if (source.toString() === resolved) return
            if (delegateUrl === "") {
                setSource("")
                return
            }
            setSource(delegateUrl, { "host": node })
        }
    }
}
