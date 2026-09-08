import QtQuick
import "../Props.js" as Props

// What a section of a panel is called.
//
// The weight and spacing a shell gives its section titles, so a panel written
// against Omega looks like the panels beside it without an author choosing a
// size.
Text {
    required property var host

    text: Props.text(host.model, "text", "")
    color: host.ink
    font.bold: true
    font.pointSize: Math.max(1, Qt.application.font.pointSize - 1)
    opacity: 0.7
    topPadding: 4
    verticalAlignment: Text.AlignVCenter
}
