import QtQuick
import "../Props.js" as Props

// Section title using theme typography.
Text {
    required property var host

    text: Props.headerText(host.model)
    // Darken the heading background without making it transparent.
    color: Qt.darker(host.ink, 1.4)
    font.family: host.fontFamily
    font.pixelSize: host.captionSize
    font.bold: true
    // Reserve vertical space for Nerd Font glyphs that extend above text bounds.
    topPadding: Math.ceil(host.captionSize * 0.15)
    verticalAlignment: Text.AlignVCenter
}
