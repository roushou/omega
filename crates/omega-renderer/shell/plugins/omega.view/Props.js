.pragma library

// Reading a node's props.
//
// A prop is a protobuf `Value`, which in JSON is a one-key object naming the
// kind: `{"stringValue": "80%"}`, `{"boolValue": true}`. The one that catches
// people is `intValue` — protobuf writes 64-bit integers as *strings*, so a
// gap of 6 arrives as "6" and using it directly lays out a NaN.
//
// The Rust side pins these shapes in a test, because a prop renamed on one
// side of this boundary and not the other is a widget that silently draws
// nothing.

function text(node, name, fallback) {
    var value = prop(node, name)
    return value && value.stringValue !== undefined ? String(value.stringValue) : fallback
}

function number(node, name, fallback) {
    var value = prop(node, name)
    if (!value || value.intValue === undefined) return fallback
    var parsed = parseInt(value.intValue, 10)
    return isNaN(parsed) ? fallback : parsed
}

function fraction(node, name, fallback) {
    var value = prop(node, name)
    return value && value.doubleValue !== undefined ? Number(value.doubleValue) : fallback
}

function flag(node, name, fallback) {
    var value = prop(node, name)
    return value && value.booleanValue !== undefined
        ? Boolean(value.booleanValue)
        : (value && value.boolValue !== undefined ? Boolean(value.boolValue) : fallback)
}

// A node's binding for an event, or null. The shape is
// `{"command": "connect", "args": [{"stringValue": "home"}]}` — and those
// args are already protobuf JSON `Value`s, which is exactly what an
// `InvokeUnit` carries, so they travel back untouched.
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
        default: return null
    }
}

// A run of numbers, as a graph's points arrive: a `Value` holding a
// `ListValue` of doubles, which protobuf JSON writes as
// `{"list": {"values": [{"doubleValue": 1.0}, …]}}`. An entry that is not a
// double is dropped rather than laid out as NaN.
function fractions(node, name) {
    var value = prop(node, name)
    if (!value || !value.list || !value.list.values) return []

    var out = []
    for (var i = 0; i < value.list.values.length; i++) {
        var held = value.list.values[i]
        if (held && held.doubleValue !== undefined) out.push(Number(held.doubleValue))
    }
    return out
}

function children(node) {
    return node && node.children ? node.children : []
}

function prop(node, name) {
    return node && node.props ? node.props[name] : undefined
}
