import QtQuick
import "../Props.js" as Props
import "../Icons.js" as Icons

// Render named Nerd Font glyphs, falling back to the name as text.
// Text advance widths determine layout inside rows.
Text {
    required property var host

    text: {
        var name = Props.iconName(host.model)
        var glyph = Icons.glyph(name)
        return glyph === "" ? name : glyph
    }
    color: host.ink
    font.family: host.fontFamily
    // Unset, an icon is drawn at the shell's icon size, which is already a
    // little larger than body text — so the fallback is that, not body.
    font.pixelSize: host.typeSize(Props.iconSize(host.model), host.iconSize)
    verticalAlignment: Text.AlignVCenter
}
