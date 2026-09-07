import QtQuick
import "Props.js" as Props

// One node of a published view tree, and its children under it.
//
// Recursive: a stack instantiates a `ViewNode` per child, which is this file
// again. QML refuses a component that names itself — "ViewNode is
// instantiated recursively" — so a child is loaded by *url* instead, which is
// resolved when it is reached rather than when this file is compiled.
//
// Five kinds, matching the SDK's vocabulary — anything else draws nothing
// rather than guessing, so a tree from a newer plugin degrades to the parts
// this shell understands instead of failing whole.
//
// A press travels the other way: a button emits `invoke`, every stack
// forwards its children's, and the widget at the top sends it to the daemon.
Item {
    id: node

    // The node object, as it arrived in the JSON line.
    required property var model
    // What text is drawn in, unless a node says otherwise. The bar passes its
    // own foreground down, so a tree is themed like everything beside it.
    property color foreground: "#ffffff"

    signal invoke(string command)

    implicitWidth: content.implicitWidth
    implicitHeight: content.implicitHeight

    Loader {
        id: content
        anchors.centerIn: parent
        sourceComponent: {
            if (!node.model) return null
            switch (node.model.type) {
                case "text": return textNode
                case "icon": return iconNode
                case "progress": return progressNode
                case "button": return buttonNode
                case "stack":
                    return Props.text(node.model, "align", "row") === "column"
                        ? columnStack
                        : rowStack
                default: return null
            }
        }
    }

    // A colour a node asked for, or the one it inherited. A theme name is
    // resolved here; anything else is taken literally, so `#ff8800` works.
    function colorOf() {
        var named = Props.text(node.model, "color", "")
        var base = named === "" ? node.foreground : themed(named)
        return Props.flag(node.model, "dim", false)
            ? Qt.rgba(base.r, base.g, base.b, 0.6)
            : base
    }

    function themed(name) {
        switch (name) {
            case "urgent": return "#e06c75"
            case "accent": return "#61afef"
            case "muted": return Qt.rgba(node.foreground.r, node.foreground.g,
                                         node.foreground.b, 0.6)
            default: return name
        }
    }

    Component {
        id: textNode
        Text {
            text: Props.text(node.model, "text", "")
            color: node.colorOf()
            font.bold: Props.flag(node.model, "bold", false)
            verticalAlignment: Text.AlignVCenter
        }
    }

    Component {
        id: iconNode
        // Until the shell has an icon set to map names through, an icon is
        // its name: honest, legible, and the mapping goes here.
        Text {
            text: Props.text(node.model, "name", "")
            color: node.colorOf()
            verticalAlignment: Text.AlignVCenter
        }
    }

    Component {
        id: progressNode
        Rectangle {
            implicitWidth: 48
            implicitHeight: 4
            radius: height / 2
            color: Qt.rgba(node.foreground.r, node.foreground.g, node.foreground.b, 0.2)

            Rectangle {
                width: parent.width
                    * Math.max(0, Math.min(1, Props.fraction(node.model, "value", 0)))
                height: parent.height
                radius: parent.radius
                color: node.colorOf()
            }
        }
    }

    Component {
        id: buttonNode
        Text {
            text: Props.text(node.model, "label", "")
            color: node.colorOf()
            font.bold: Props.flag(node.model, "bold", false)
            verticalAlignment: Text.AlignVCenter

            MouseArea {
                anchors.fill: parent
                enabled: Props.text(node.model, "command", "") !== ""
                cursorShape: Qt.PointingHandCursor
                // The command is this button's own, so a press reaches the
                // plugin that drew it and no other.
                onClicked: node.invoke(Props.text(node.model, "command", ""))
            }
        }
    }

    Component {
        id: rowStack
        Row {
            spacing: Props.number(node.model, "gap", 0)
            Repeater {
                model: Props.children(node.model)
                delegate: childNode
            }
        }
    }

    Component {
        id: columnStack
        Column {
            spacing: Props.number(node.model, "gap", 0)
            Repeater {
                model: Props.children(node.model)
                delegate: childNode
            }
        }
    }

    // One child, loaded by url so the recursion is resolved at runtime. The
    // node's properties are given at construction because `model` is
    // required, and its presses are forwarded up as they arrive.
    Component {
        id: childNode
        Loader {
            id: child
            required property var modelData

            Component.onCompleted: setSource("ViewNode.qml", {
                "model": child.modelData,
                "foreground": node.foreground
            })

            Connections {
                target: child.item
                function onInvoke(command) { node.invoke(command) }
            }
        }
    }
}
