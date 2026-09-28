//! Generate named QML property accessors from [`NodeKind`].
//! The checked-in `Props.js` must match this generator; regenerate with
//! `OMEGA_REGENERATE=1 cargo test -p omega-renderer`.

use std::fmt::Write as _;

use omega_proto::{
    NodeKind, Prop, PropKind,
    ui::{DefaultValue, Payload, SHARED},
};

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

        out.push_str("\nvar contracts = {\n");
        for kind in NodeKind::ALL {
            let _ = write!(
                out,
                "    {:?}: {{children: {:?}, events: {{",
                kind.name(),
                kind.children().name()
            );
            for event in kind.events() {
                let _ = write!(out, "{:?}: {},", event.name, Self::payload(event.payload));
            }
            out.push_str("}},\n");
        }
        out.push_str("}\n");
        out.push_str(Self::EPILOGUE);
        out
    }

    fn emit(out: &mut String, accessor: &str, prop: &Prop) {
        let _ = write!(
            out,
            "\nfunction {accessor}(node) {{\n    return {}(node, {:?}, {})\n}}\n",
            Self::reader(prop.kind),
            prop.name,
            Self::fallback(prop.fallback)
        );
    }

    fn reader(kind: PropKind) -> &'static str {
        match kind {
            PropKind::Text => "readText",
            PropKind::Number => "readNumber",
            PropKind::Fraction => "readFraction",
            PropKind::Flag => "readFlag",
            PropKind::Fractions => "readFractions",
        }
    }

    fn fallback(value: DefaultValue) -> String {
        match value {
            DefaultValue::Absent => "null".into(),
            DefaultValue::Text(value) => format!("{value:?}"),
            DefaultValue::Integer(value) => value.to_string(),
            DefaultValue::Fraction(value) => value.to_string(),
            DefaultValue::Flag(value) => value.to_string(),
            DefaultValue::EmptyList => "[]".into(),
        }
    }

    fn payload(payload: Payload) -> String {
        Self::payload_default(payload, None)
    }

    fn payload_default(payload: Payload, fallback: Option<DefaultValue>) -> String {
        let fields: Vec<_> = payload
            .fields()
            .iter()
            .map(|field| {
                format!(
                    "{:?}:{}",
                    field.name,
                    Self::payload_default(field.payload, field.fallback)
                )
            })
            .collect();
        format!(
            "{{kind:{:?},fields:{{{}}},fallback:{}}}",
            payload.name(),
            fields.join(","),
            fallback
                .map(Self::fallback)
                .unwrap_or_else(|| "undefined".into())
        )
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
"#;
}
