import QtQuick
import "../Props.js" as Props

// A run of text.
Text {
    required property var host

    text: Props.textText(host.model)
    color: host.ink
    font.family: host.fontFamily
    font.pixelSize: host.typeSize(Props.textSize(host.model), host.fontSize)
    font.bold: Props.bold(host.model)
    verticalAlignment: Text.AlignVCenter
}
