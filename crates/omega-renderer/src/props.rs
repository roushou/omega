//! Generating the shell's prop readers.
//!
//! QML cannot be compiled against the SDK's types, so the boundary between
//! what a unit publishes and what a shell reads has always been two lists of
//! strings that had to agree. A test pinned the shapes the wire carries,
//! which catches a prop that changed shape — and never caught a prop the
//! shell reads by a name nothing publishes, or one published that nothing
//! reads. Both are silent: a widget that draws an empty string.
//!
//! So the readers are not written. [`NodeKind`] is the vocabulary and this
//! emits `Props.js` from it, one named accessor per (kind, prop): a shell
//! calls `Props.textText(node)`, never `Props.text(node, "text", "")`, and
//! the only place a prop's name is spelled is the table.
//!
//! The result is checked in rather than built. The renderer's tree travels
//! inside the binary and is asserted against the directory on disk, the QML
//! linter runs over it, and a reviewer should see what changed — none of
//! which is true of a file that only exists in `OUT_DIR`. A test regenerates
//! and compares; `OMEGA_REGENERATE=1 cargo test -p omega-renderer` writes it.

use std::fmt::Write as _;

use omega_proto::{NodeKind, Prop, ui::SHARED};

/// The generated reader module.
#[derive(Debug)]
pub struct Props;

impl Props {
    /// Where the generated file belongs in the shell tree.
    pub const FILE: &'static str = "Props.js";

    /// The accessor a shell calls for one of a kind's own props.
    ///
    /// Named for the kind as well as the prop, because one name can mean two
    /// things: `value` is a fraction on a slider and a string on a field, and
    /// a single `value` reader would have to guess which.
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

    /// The whole file.
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

    /// The hand-written half: decoding a `Value`, and the parts of a node
    /// that are not props at all.
    ///
    /// Five decoders and three helpers, because they are about the *wire*
    /// rather than the vocabulary — protobuf JSON's shape does not change
    /// when a node kind is added.
    const PREAMBLE: &'static str = r#".pragma library

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
"#;

    /// What a node carries besides props: its children, and what it does.
    const EPILOGUE: &'static str = r#"
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
"#;
}
