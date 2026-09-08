import QtQuick
import "../Props.js" as Props

// A run of text.
Text {
    required property var host

    text: Props.text(host.model, "text", "")
    color: host.ink
    font.bold: Props.flag(host.model, "bold", false)
    verticalAlignment: Text.AlignVCenter
}
