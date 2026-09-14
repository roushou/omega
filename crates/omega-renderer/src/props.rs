//! Generate named QML property accessors from [`NodeKind`].
//! The checked-in `Props.js` must match this generator; regenerate with
//! `OMEGA_REGENERATE=1 cargo test -p omega-renderer`.

use std::fmt::Write as _;

use omega_proto::{NodeKind, Prop, ui::SHARED};

/// The generated reader module.
#[derive(Debug)]
pub struct Props;

impl Props {
    /// Where the generated file belongs in the shell tree.
    pub const FILE: &'static str = "Props.js";

    /// Generate a kind-qualified property accessor.
    /// Names must distinguish fields with different encodings, such as slider and field values.
    pub fn accessor(kind: NodeKind, prop: &Prop) -> String {
        format!("{}{}", kind.name(), Self::capitalize(prop.name))
    }

    /// The accessor for a prop any node may carry. Unprefixed: there is one
    /// `color` and every kind reads it the same way.
    pub fn shared_accessor(prop: &Prop) -> String {
        prop.name.to_string()
    }

    fn capitalize(name: &str) -> String {
        let mut chars = name.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    }

    /// Every accessor the generated file defines.
    pub fn accessors() -> Vec<String> {
        let mut names: Vec<String> = SHARED.iter().map(Self::shared_accessor).collect();
        for kind in NodeKind::ALL {
            names.extend(kind.props().iter().map(|prop| Self::accessor(*kind, prop)));
        }
        names
    }

    /// Generate the complete JavaScript file.
    pub fn generate() -> String {
        let mut out = String::new();
        out.push_str(Self::PREAMBLE);

        out.push_str("\n// ---- props every node may carry ----\n");
        for prop in SHARED {
            Self::emit(&mut out, &Self::shared_accessor(prop), prop);
        }

        for kind in NodeKind::ALL {
            if kind.props().is_empty() {
                continue;
            }
            let _ = write!(out, "\n// ---- {kind} ----\n");
            for prop in kind.props() {
                Self::emit(&mut out, &Self::accessor(*kind, prop), prop);
            }
        }

        out.push_str(Self::EPILOGUE);
        out
    }

    fn emit(out: &mut String, accessor: &str, prop: &Prop) {
        let _ = write!(
            out,
            "\nfunction {accessor}(node) {{\n    return {}(node, {:?}, {})\n}}\n",
            prop.kind.reader(),
            prop.name,
            prop.fallback
        );
    }

    /// Shared protobuf JSON decoders and non-property node helpers.
    const PREAMBLE: &'static str = r#".pragma library

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
"#;

    /// What a node carries besides props: its children, and what it does.
    const EPILOGUE: &'static str = r#"
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
"#;
}
