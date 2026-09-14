import QtQuick
import "../Props.js" as Props

// What a section of a panel is called.
//
// The weight and spacing a shell gives its section titles, so a panel written
// against Omega looks like the panels beside it without an author choosing a
// size.
Text {
    required property var host

    text: Props.headerText(host.model)
    // Darkened rather than made transparent, which is what
    // `PanelSectionHeader` does: opacity would also fade whatever shows
    // through from behind the panel.
    color: Qt.darker(host.ink, 1.4)
    font.family: host.fontFamily
    font.pixelSize: host.captionSize
    font.bold: true
    // Nerd Font glyphs paint above the box `Text` reserves, and a header at
    // the top of a clipping list loses the overshoot to the clip. The kit
    // reserves it here for the same reason.
    topPadding: Math.ceil(host.captionSize * 0.15)
    verticalAlignment: Text.AlignVCenter
}
