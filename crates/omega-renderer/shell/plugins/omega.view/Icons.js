.pragma library

// Icon names, and the glyphs a bar draws them as.
//
// Omarchy's shell draws icons as characters, not images: the bar's font is
// the fontconfig alias `omarchy font set` writes, which resolves to a Nerd
// Font. So an icon is a lookup from a name a unit can write to a codepoint
// that font carries.
//
// These are the Font Awesome block (U+F000..U+F2FF), which is the oldest and
// most widely present part of every Nerd Font patch — a glyph from here draws
// under JetBrainsMono, CaskaydiaCove, Hack and the rest alike. Written as
// escapes so this file stays ASCII and survives any encoding it is copied
// through.
//
// A name that is not here is drawn as itself by `ViewNode.qml`, so a unit
// asking for an icon this shell has never heard of shows a legible word
// rather than a blank space or a replacement box.

var GLYPHS = {
    // Power.
    "battery": "\uf240",
    "battery-full": "\uf240",
    "battery-three-quarters": "\uf241",
    "battery-half": "\uf242",
    "battery-quarter": "\uf243",
    "battery-empty": "\uf244",
    "plug": "\uf1e6",
    "power": "\uf011",

    // Network.
    "wifi": "\uf1eb",
    "globe": "\uf0ac",
    "link": "\uf0c1",
    "bluetooth": "\uf293",
    "download": "\uf019",
    "upload": "\uf093",

    // Sound.
    "volume": "\uf028",
    "volume-up": "\uf028",
    "volume-down": "\uf027",
    "volume-off": "\uf026",
    "headphones": "\uf025",
    "microphone": "\uf130",
    "microphone-off": "\uf131",
    "music": "\uf001",

    // Machine.
    "cpu": "\uf2db",
    "thermometer": "\uf2c7",
    "keyboard": "\uf11c",
    "camera": "\uf030",
    "terminal": "\uf120",
    "cog": "\uf013",

    // Time.
    "clock": "\uf017",
    "calendar": "\uf073",
    "sun": "\uf185",
    "moon": "\uf186",

    // Status.
    "bell": "\uf0f3",
    "warning": "\uf071",
    "check": "\uf00c",
    "close": "\uf00d",
    "refresh": "\uf021",
    "lock": "\uf023",
    "search": "\uf002",
    "star": "\uf005",
    "heart": "\uf004",

    // Places and people.
    "home": "\uf015",
    "user": "\uf007",
    "folder": "\uf07b",
    "envelope": "\uf0e0",
    "trash": "\uf1f8"
};

// The glyph for a name, or "" when this shell has no glyph for it.
function glyph(name) {
    return Object.prototype.hasOwnProperty.call(GLYPHS, name) ? GLYPHS[name] : ""
}

// Every name this shell draws, sorted — what the SDK's documentation and its
// test read, so the two cannot drift.
function names() {
    return Object.keys(GLYPHS).sort()
}
