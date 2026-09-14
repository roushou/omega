.pragma library

// GENERATED icon lookup from `omega-proto` by `omega_renderer::Icons`.
// Edit the source table and run `OMEGA_REGENERATE=1 cargo test -p omega-omarchy --test renderer`.
// Glyphs use the Nerd Font Font Awesome range; unknown names display as text.

var GLYPHS = {
    "battery": "\uf240",
    "battery-full": "\uf240",
    "battery-three-quarters": "\uf241",
    "battery-half": "\uf242",
    "battery-quarter": "\uf243",
    "battery-empty": "\uf244",
    "plug": "\uf1e6",
    "power": "\uf011",
    "wifi": "\uf1eb",
    "globe": "\uf0ac",
    "link": "\uf0c1",
    "bluetooth": "\uf293",
    "download": "\uf019",
    "upload": "\uf093",
    "volume": "\uf028",
    "volume-up": "\uf028",
    "volume-down": "\uf027",
    "volume-off": "\uf026",
    "headphones": "\uf025",
    "microphone": "\uf130",
    "microphone-off": "\uf131",
    "play": "\uf04b",
    "pause": "\uf04c",
    "stop": "\uf04d",
    "next": "\uf051",
    "previous": "\uf048",
    "music": "\uf001",
    "cpu": "\uf2db",
    "thermometer": "\uf2c7",
    "keyboard": "\uf11c",
    "camera": "\uf030",
    "terminal": "\uf120",
    "cog": "\uf013",
    "clock": "\uf017",
    "calendar": "\uf073",
    "sun": "\uf185",
    "moon": "\uf186",
    "bell": "\uf0f3",
    "warning": "\uf071",
    "check": "\uf00c",
    "close": "\uf00d",
    "refresh": "\uf021",
    "lock": "\uf023",
    "search": "\uf002",
    "star": "\uf005",
    "heart": "\uf004",
    "home": "\uf015",
    "user": "\uf007",
    "folder": "\uf07b",
    "envelope": "\uf0e0",
    "trash": "\uf1f8",
}

// The glyph for a name, or "" for one this shell does not draw.
function glyph(name) {
    return Object.prototype.hasOwnProperty.call(GLYPHS, name) ? GLYPHS[name] : ""
}

// Every name this shell draws, sorted.
function names() {
    return Object.keys(GLYPHS).sort()
}
