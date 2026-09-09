import QtQuick
import "../Props.js" as Props
import "../Icons.js" as Icons

// A named icon, as a Nerd Font glyph.
//
// A name this shell has no glyph for is drawn as itself, so a typo — or a
// unit written for a richer shell — reads as a legible word rather than a
// blank space.
//
// Drawn as plain `Text`, not `qs.Ui`'s `OpticalGlyph`: that corrects the
// horizontal bearing of a glyph centred in a fixed slot, and these sit in a
// `Row` that lays out advance widths. Text also brings its own implicit size,
// which an `Item` does not.
Text {
    required property var host

    text: {
        var name = Props.iconName(host.model)
        var glyph = Icons.glyph(name)
        return glyph === "" ? name : glyph
    }
    color: host.ink
    font.family: host.fontFamily
    font.pixelSize: host.iconSize
    verticalAlignment: Text.AlignVCenter
}
