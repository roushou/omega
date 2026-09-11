.pragma library

// Reading a node's props. GENERATED from `omega-proto`'s node table by
// `omega_renderer::Props` — do not edit; add the prop there and regenerate
// with `OMEGA_REGENERATE=1 cargo test -p omega-renderer`.
//
// A prop is a protobuf `Value`, which in JSON is a one-key object naming the
// kind: `{"stringValue": "80%"}`, `{"boolValue": true}`. The one that catches
// people is `intValue` — protobuf writes 64-bit integers as *strings*, so a
// gap of 6 arrives as "6" and using it directly lays out a NaN. That is why
// a number and a fraction have separate readers below, and why nothing here
// takes a prop name from its caller: a shell that could spell a name could
// spell it wrong, and an unread prop draws an empty string rather than
// failing.

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

// A run of numbers: a `Value` holding a `ListValue` of doubles, which
// protobuf JSON writes as `{"list": {"values": [{"doubleValue": 1.0}, ...]}}`.
// An entry that is not a double is dropped rather than laid out as NaN.
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

function bold(node) {
    return readFlag(node, "bold", false)
}

function dim(node) {
    return readFlag(node, "dim", false)
}

function pad(node) {
    return readNumber(node, "pad", 0)
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

// ---- slider ----

function sliderValue(node) {
    return readFraction(node, "value", 0)
}

// ---- toggle ----

function toggleOn(node) {
    return readFlag(node, "on", false)
}

// ---- form ----

function formLabel(node) {
    return readText(node, "label", "")
}

// ---- field ----

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

// ---- list ----

function listGap(node) {
    return readNumber(node, "gap", 0)
}

// ---- group ----

function groupSelected(node) {
    return readText(node, "selected", "")
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

// ---- what a node is, besides its props ----

// A node's binding for an event, or null. The shape is
// `{"command": "connect", "args": [{"stringValue": "home"}]}` — and those
// args are already protobuf JSON `Value`s, which is exactly what an
// `InvokeUnit` carries, so they travel back untouched.
//
// Not generated: an event is not a prop. `ViewNode.events` is its own map on
// the wire, so that a shell reads behaviour from one place rather than
// sniffing prop names for it.
function bind(node, event) {
    if (!node || !node.events) return null
    var bound = node.events[event]
    return bound && bound.command ? bound : null
}

// A JS value as a protobuf JSON `Value`, for a control reporting what the
// user did. Doubles and booleans travel as themselves; `intValue` is the one
// protobuf JSON writes as a *string*, which is why a whole number is sent as
// a double unless a caller asks otherwise — a control's value is a reading,
// not a count.
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
                if (typeof value[key] !== "string") return null
                fields[key] = { stringValue: value[key] }
            }
            return { map: { entries: fields } }
        }
        default: return null
    }
}

function children(node) {
    return node && node.children ? node.children : []
}
