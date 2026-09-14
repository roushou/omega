.pragma library

// This adapter uses logical Qt keys. Native scan codes are backend-specific.
function identity(key) {
    switch (key) {
    case Qt.Key_Escape: return "key:Escape"
    case Qt.Key_Return: case Qt.Key_Enter: return "key:Enter"
    case Qt.Key_Tab: case Qt.Key_Backtab: return "key:Tab"
    case Qt.Key_Backspace: return "key:Backspace"
    case Qt.Key_Delete: return "key:Delete"
    case Qt.Key_Insert: return "key:Insert"
    case Qt.Key_Home: return "key:Home"
    case Qt.Key_End: return "key:End"
    case Qt.Key_PageUp: return "key:PageUp"
    case Qt.Key_PageDown: return "key:PageDown"
    case Qt.Key_Left: return "key:ArrowLeft"
    case Qt.Key_Right: return "key:ArrowRight"
    case Qt.Key_Up: return "key:ArrowUp"
    case Qt.Key_Down: return "key:ArrowDown"
    case Qt.Key_Space: return "key:Space"
    }
    if (key >= Qt.Key_F1 && key <= Qt.Key_F12) return "key:F" + (key - Qt.Key_F1 + 1)
    if (key >= 0x21 && key <= 0x10ffff && !(key >= 0xd800 && key <= 0xdfff))
        return "char:" + String.fromCodePoint(key).toLowerCase()
    return ""
}
function modifiers(native) {
    var bits = 0
    if (native & Qt.ControlModifier) bits |= 1
    if (native & Qt.ShiftModifier) bits |= 2
    if (native & Qt.AltModifier) bits |= 4
    if (native & Qt.MetaModifier) bits |= 8
    if (native & Qt.GroupSwitchModifier) bits |= 16
    return bits
}
function matches(binding, key, mods, release, repeat) {
    return binding.key === key && Number(binding.modifiers || 0) === mods
        && !!binding.release === release && (!repeat || !!binding.repeat)
}
function resolve(bindings, event, release) {
    var key = identity(event.key)
    var mods = modifiers(event.modifiers)
    if (!key) return null
    for (var i = 0; i < bindings.length; ++i)
        if (matches(bindings[i], key, mods, release, !!event.isAutoRepeat)) return bindings[i]
    return null
}
function composing(item) {
    for (var current = item; current; current = current.parent)
        if (current.inputMethodComposing === true || current.composing === true) return true
    return false
}
