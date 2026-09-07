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

function children(node) {
    return node && node.children ? node.children : []
}

function prop(node, name) {
    return node && node.props ? node.props[name] : undefined
}
