.pragma library

// GENERATED property readers from `omega-proto` by `omega_renderer::Props`.
// Edit the source table and run `OMEGA_REGENERATE=1 cargo test -p omega-renderer`.
// Protobuf JSON encodes int64 values as strings; numeric readers must convert them.

// ---- decoding one Value ----

function readText(node, name, fallback) {
    var value = prop(node, name)
    return value && value.stringValue !== undefined ? String(value.stringValue) : fallback
}

function readNumber(node, name, fallback) {
    var value = prop(node, name)
    if (!value || value.intValue === undefined) return fallback
    var parsed = parseInt(value.intValue, 10)
    return isNaN(parsed) ? fallback : parsed
}

function readFraction(node, name, fallback) {
    var value = prop(node, name)
    return value && value.doubleValue !== undefined ? Number(value.doubleValue) : fallback
}

function readFlag(node, name, fallback) {
    var value = prop(node, name)
    return value && value.boolValue !== undefined ? Boolean(value.boolValue) : fallback
}

// Decode a protobuf JSON list of doubles, skipping non-double entries.
function readFractions(node, name, fallback) {
    var value = prop(node, name)
    if (!value || !value.list || !value.list.values) return fallback

    var out = []
    for (var i = 0; i < value.list.values.length; i++) {
        var held = value.list.values[i]
        if (held && held.doubleValue !== undefined) out.push(Number(held.doubleValue))
    }
    return out
}

function prop(node, name) {
    return node && node.props ? node.props[name] : undefined
}

// ---- props every node may carry ----

function color(node) {
    return readText(node, "color", "")
}

function emphasis(node) {
    return readText(node, "emphasis", "")
}

function tone(node) {
    return readText(node, "tone", "")
}

function bold(node) {
    return readFlag(node, "bold", false)
}

function dim(node) {
    return readFlag(node, "dim", false)
}

function pad(node) {
    return readNumber(node, "pad", 0)
}

function selection_key(node) {
    return readText(node, "selection_key", null)
}

function tooltip(node) {
    return readText(node, "tooltip", "")
}

function disabled(node) {
    return readFlag(node, "disabled", false)
}

function busy(node) {
    return readFlag(node, "busy", false)
}

function width(node) {
    return readNumber(node, "width", 0)
}

function height(node) {
    return readNumber(node, "height", 0)
}

function fill(node) {
    return readFlag(node, "fill", false)
}

// ---- stack ----

function stackAlign(node) {
    return readText(node, "align", "row")
}

function stackGap(node) {
    return readNumber(node, "gap", 0)
}

// ---- text ----

function textText(node) {
    return readText(node, "text", "")
}

function textSize(node) {
    return readText(node, "size", "body")
}

// ---- icon ----

function iconName(node) {
    return readText(node, "name", "")
}

function iconSize(node) {
    return readText(node, "size", "")
}

// ---- header ----

function headerText(node) {
    return readText(node, "text", "")
}

// ---- grid ----

function gridColumns(node) {
    return readNumber(node, "columns", 1)
}

function gridGap(node) {
    return readNumber(node, "gap", 0)
}

// ---- button ----

function buttonLabel(node) {
    return readText(node, "label", "")
}

function buttonIcon(node) {
    return readText(node, "icon", "")
}

function buttonFlat(node) {
    return readFlag(node, "flat", false)
}

// ---- slider ----

function sliderValue(node) {
    return readFraction(node, "value", 0)
}

// ---- toggle ----

function toggleOn(node) {
    return readFlag(node, "on", false)
}

// ---- checkbox ----

function checkboxOn(node) {
    return readFlag(node, "on", false)
}

function checkboxLabel(node) {
    return readText(node, "label", "")
}

// ---- form ----

function formLabel(node) {
    return readText(node, "label", "")
}

// ---- field ----

function fieldSize(node) {
    return readText(node, "size", "")
}

function fieldNavigation(node) {
    return readText(node, "navigation", "")
}

function fieldControlled(node) {
    return readFlag(node, "controlled", false)
}

function fieldEdit_revision(node) {
    return readNumber(node, "edit_revision", 0)
}

function fieldReset_revision(node) {
    return readNumber(node, "reset_revision", 0)
}

function fieldAutofocus(node) {
    return readFlag(node, "autofocus", false)
}

function fieldLabel(node) {
    return readText(node, "label", "")
}

function fieldHelp(node) {
    return readText(node, "help", "")
}

function fieldName(node) {
    return readText(node, "name", "")
}

function fieldPlaceholder(node) {
    return readText(node, "placeholder", "")
}

function fieldSecret(node) {
    return readFlag(node, "secret", false)
}

function fieldValue(node) {
    return readText(node, "value", "")
}

function fieldNumeric(node) {
    return readFlag(node, "numeric", false)
}

function fieldMin(node) {
    return readFraction(node, "min", 0)
}

function fieldMax(node) {
    return readFraction(node, "max", 0)
}

function fieldStep(node) {
    return readFraction(node, "step", 0)
}

// ---- list ----

function listGap(node) {
    return readNumber(node, "gap", 0)
}

function listSelected(node) {
    return readText(node, "selected", null)
}

// ---- group ----

function groupSelected(node) {
    return readText(node, "selected", null)
}

// ---- dropdown ----

function dropdownSelected(node) {
    return readText(node, "selected", null)
}

function dropdownPlaceholder(node) {
    return readText(node, "placeholder", "")
}

// ---- disclosure ----

function disclosureTitle(node) {
    return readText(node, "title", "")
}

function disclosureOpen(node) {
    return readFlag(node, "open", false)
}

// ---- dialog ----

function dialogTitle(node) {
    return readText(node, "title", "")
}

function dialogBody(node) {
    return readText(node, "body", "")
}

function dialogConfirm(node) {
    return readText(node, "confirm", "")
}

function dialogCancel(node) {
    return readText(node, "cancel", "")
}

// ---- progress ----

function progressValue(node) {
    return readFraction(node, "value", 0)
}

// ---- graph ----

function graphPoints(node) {
    return readFractions(node, "points", [])
}

function graphLow(node) {
    return readFraction(node, "low", 0)
}

function graphHigh(node) {
    return readFraction(node, "high", 0)
}

// ---- image ----

function imageSource(node) {
    return readText(node, "source", "")
}

// ---- badge ----

function badgeCount(node) {
    return readNumber(node, "count", 0)
}

function badgeHidden_when_zero(node) {
    return readFlag(node, "hidden_when_zero", false)
}

// ---- keycap ----

function keycapLabel(node) {
    return readText(node, "label", "")
}

// ---- status ----

function statusTitle(node) {
    return readText(node, "title", "")
}

function statusMessage(node) {
    return readText(node, "message", "")
}

function statusIcon(node) {
    return readText(node, "icon", "")
}

// ---- what a node is, besides its props ----

// Return the node's event binding or null. Arguments retain protobuf JSON encoding.
function bind(node, event) {
    if (!node || !node.events) return null
    var bound = node.events[event]
    return bound && (bound.command || (bound.local && bound.local !== "0")) ? bound : null
}

// Encode a JavaScript control value as a protobuf JSON Value.
// Numbers use doubleValue; explicit int64 values require string encoding.
function encode(value) {
    switch (typeof value) {
        case "boolean": return { "boolValue": value }
        case "number": return { "doubleValue": value }
        case "string": return { "stringValue": value }
        case "object": {
            if (value === null || Array.isArray(value)) return null
            var fields = Object.create(null)
            for (var key in value) {
                if (!Object.prototype.hasOwnProperty.call(value, key)) continue
                var item = value[key]
                fields[key] = typeof item === "number" && Number.isSafeInteger(item)
                    ? { intValue: String(item) } : encode(item)
                if (fields[key] === null) return null
            }
            return { map: { entries: fields } }
        }
        default: return null
    }
}

function children(node) {
    return node && node.children ? node.children : []
}
