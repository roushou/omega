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

// ---- textarea ----

function textareaRows(node) {
    return readNumber(node, "rows", 4)
}

function textareaLabel(node) {
    return readText(node, "label", "")
}

function textareaPlaceholder(node) {
    return readText(node, "placeholder", "")
}

function textareaControlled(node) {
    return readFlag(node, "controlled", false)
}

function textareaEdit_revision(node) {
    return readNumber(node, "edit_revision", 0)
}

function textareaReset_revision(node) {
    return readNumber(node, "reset_revision", 0)
}

function textareaAutofocus(node) {
    return readFlag(node, "autofocus", false)
}

function textareaValue(node) {
    return readText(node, "value", "")
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

function imageFit(node) {
    return readText(node, "fit", "contain")
}

// ---- viewport ----

function viewportZoom(node) {
    return readFraction(node, "zoom", 1)
}

function viewportOffset_x(node) {
    return readFraction(node, "offset_x", 0)
}

function viewportOffset_y(node) {
    return readFraction(node, "offset_y", 0)
}

function viewportFit(node) {
    return readText(node, "fit", "contain")
}

function viewportRevision(node) {
    return readNumber(node, "revision", 0)
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

var contracts = {
    "stack": {children: "many", events: {}},
    "text": {children: "none", events: {}},
    "icon": {children: "none", events: {}},
    "header": {children: "none", events: {}},
    "separator": {children: "none", events: {}},
    "spacer": {children: "none", events: {}},
    "grid": {children: "many", events: {}},
    "button": {children: "none", events: {"press": {kind:"none",fields:{},fallback:undefined},}},
    "slider": {children: "none", events: {"change": {kind:"percent",fields:{},fallback:undefined},}},
    "toggle": {children: "none", events: {"change": {kind:"flag",fields:{},fallback:undefined},}},
    "checkbox": {children: "none", events: {"change": {kind:"flag",fields:{},fallback:undefined},}},
    "form": {children: "fields", events: {"submit": {kind:"form",fields:{},fallback:undefined},}},
    "field": {children: "none", events: {"submit": {kind:"text",fields:{},fallback:undefined},"change": {kind:"edit",fields:{"text":{kind:"text",fields:{},fallback:undefined},"revision":{kind:"unsigned",fields:{},fallback:undefined},"reset":{kind:"unsigned",fields:{},fallback:undefined}},fallback:undefined},}},
    "textarea": {children: "none", events: {"change": {kind:"edit",fields:{"text":{kind:"text",fields:{},fallback:undefined},"revision":{kind:"unsigned",fields:{},fallback:undefined},"reset":{kind:"unsigned",fields:{},fallback:undefined}},fallback:undefined},}},
    "list": {children: "many", events: {"select": {kind:"text",fields:{},fallback:undefined},"activate": {kind:"text",fields:{},fallback:undefined},}},
    "group": {children: "many", events: {"select": {kind:"text",fields:{},fallback:undefined},}},
    "dropdown": {children: "many", events: {"select": {kind:"text",fields:{},fallback:undefined},}},
    "disclosure": {children: "many", events: {"toggle": {kind:"flag",fields:{},fallback:undefined},}},
    "dialog": {children: "none", events: {"confirm": {kind:"none",fields:{},fallback:undefined},"cancel": {kind:"none",fields:{},fallback:undefined},"dismiss": {kind:"none",fields:{},fallback:undefined},}},
    "progress": {children: "none", events: {}},
    "graph": {children: "none", events: {}},
    "image": {children: "none", events: {"wheel": {kind:"fraction",fields:{},fallback:undefined},}},
    "viewport": {children: "canvas", events: {"wheel": {kind:"gesture",fields:{"zoom":{kind:"real",fields:{},fallback:1},"offset_x":{kind:"real",fields:{},fallback:0},"offset_y":{kind:"real",fields:{},fallback:0},"x":{kind:"real",fields:{},fallback:0},"y":{kind:"real",fields:{},fallback:0},"dx":{kind:"real",fields:{},fallback:0},"dy":{kind:"real",fields:{},fallback:0}},fallback:undefined},"drag": {kind:"gesture",fields:{"zoom":{kind:"real",fields:{},fallback:1},"offset_x":{kind:"real",fields:{},fallback:0},"offset_y":{kind:"real",fields:{},fallback:0},"x":{kind:"real",fields:{},fallback:0},"y":{kind:"real",fields:{},fallback:0},"dx":{kind:"real",fields:{},fallback:0},"dy":{kind:"real",fields:{},fallback:0}},fallback:undefined},"pinch": {kind:"gesture",fields:{"zoom":{kind:"real",fields:{},fallback:1},"offset_x":{kind:"real",fields:{},fallback:0},"offset_y":{kind:"real",fields:{},fallback:0},"x":{kind:"real",fields:{},fallback:0},"y":{kind:"real",fields:{},fallback:0},"dx":{kind:"real",fields:{},fallback:0},"dy":{kind:"real",fields:{},fallback:0}},fallback:undefined},}},
    "badge": {children: "none", events: {}},
    "keycap": {children: "none", events: {}},
    "status": {children: "none", events: {}},
    "scroll": {children: "many", events: {}},
}

// ---- what a node is, besides its props ----

// Return the node's event binding or null. Arguments retain protobuf JSON encoding.
function bind(node, event) {
    if (!node || !node.events) return null
    var bound = node.events[event]
    return bound && (bound.command || (bound.local && bound.local !== "0")) ? bound : null
}

// Select the encoding from the node and event, never JavaScript numeric nesting.
// null rejects malformed/unknown events; undefined is a valid no-value event.
function encodeEvent(node, event, value) {
    if (!node) return null
    var shortcuts = node.shortcuts || []
    for (var i = 0; i < shortcuts.length; i++) {
        if (shortcuts[i].event === event) return encodePayload({kind: "none"}, value)
    }
    var contract = contracts[node.type]
    var shape = contract && contract.events[event]
    return shape ? encodePayload(shape, value) : null
}

function encodePayload(shape, value) {
    if (value === undefined && shape.fallback !== undefined) value = shape.fallback
    switch (shape.kind) {
        case "none": return value === undefined ? undefined : null
        case "text": return typeof value === "string" ? {stringValue: value} : null
        case "flag": return typeof value === "boolean" ? {boolValue: value} : null
        case "unsigned": return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= 4294967295
            ? {intValue: String(value)} : null
        case "percent": return typeof value === "number" && value >= 0 && value <= 1 ? {doubleValue: value} : null
        case "fraction":
        case "real": return typeof value === "number" && Number.isFinite(value) ? {doubleValue: value} : null
        case "edit":
        case "gesture":
        case "form": {
            if (!value || typeof value !== "object" || Array.isArray(value)) return null
            var fields = Object.create(null)
            var names = shape.kind === "form" ? Object.keys(value) : Object.keys(shape.fields)
            for (var i = 0; i < names.length; i++) {
                var key = names[i]
                var held = Object.prototype.hasOwnProperty.call(value, key) ? value[key] : undefined
                var field = encodePayload(shape.kind === "form" ? {kind: "text"} : shape.fields[key], held)
                if (field === null) return null
                fields[key] = field
            }
            return {map: {entries: fields}}
        }
        default: return null
    }
}

function children(node) {
    return node && node.children ? node.children : []
}
