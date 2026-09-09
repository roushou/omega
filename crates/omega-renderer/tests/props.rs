//! The generated prop readers, and whether the shell and the vocabulary
//! still describe the same thing.
//!
//! `Props.js` is generated from `omega-proto`'s node table, so a shell cannot
//! read a prop by a name nothing publishes. That closes one direction. These
//! close the other two: a prop declared and drawn by nothing, and an accessor
//! called that was never generated. All three used to be silent — a widget
//! that lays out an empty string is indistinguishable from one whose unit had
//! nothing to say.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use omega_proto::NodeKind;
use omega_proto::ui::SHARED;
use omega_renderer::Props;

/// The checkout this test is running inside.
fn shell() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/plugins/omega.view")
}

fn generated_path() -> PathBuf {
    shell().join(Props::FILE)
}

/// Every `.qml` in the tree, and the QML that is not a node too.
fn qml_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![shell()];

    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the shell tree is on disk") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "qml") {
                files.push(path);
            }
        }
    }

    files.sort();
    files
}

/// Every `Props.<name>` a QML file calls.
fn called() -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    for file in qml_files() {
        let source = std::fs::read_to_string(&file).expect("a readable qml file");
        let mut rest = source.as_str();

        while let Some(at) = rest.find("Props.") {
            rest = &rest[at + "Props.".len()..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            // A call, not `import "Props.js"` — the module names itself the
            // same way a reader on it does.
            if end > 0 && rest[end..].starts_with('(') {
                names.insert(rest[..end].to_string());
            }
        }
    }

    names
}

/// Accessors that exist for the wire rather than for the vocabulary, so no
/// prop declares them and nothing generates them.
const HAND_WRITTEN: &[&str] = &["bind", "encode", "children", "prop"];

/// Props declared in the vocabulary that no shell draws yet.
///
/// A hole is a line here, with a reason, or the test fails — the same bargain
/// `omega-brokers`' coverage test makes. An entry is not permission to leave
/// it: a unit calling `.tooltip(…)` today is publishing a string on every
/// render that nothing will ever show.
const NOT_DRAWN: &[&str] = &[];

#[test]
fn the_checked_in_readers_are_what_the_table_generates() {
    let generated = Props::generate();
    let path = generated_path();

    // The escape hatch, so the fix for a failure below is a command rather
    // than hand-editing a generated file to match.
    if std::env::var_os("OMEGA_REGENERATE").is_some() {
        std::fs::write(&path, &generated).expect("the shell tree is writable");
        return;
    }

    let on_disk = std::fs::read_to_string(&path).expect("Props.js is checked in");
    assert_eq!(
        on_disk,
        generated,
        "{} is not what the node table generates. \
         Run `OMEGA_REGENERATE=1 cargo test -p omega-renderer`.",
        Props::FILE
    );
}

#[test]
fn every_accessor_a_shell_calls_is_one_that_exists() {
    // The failure this replaces: a shell asking for a prop by a name nothing
    // publishes read `undefined`, took the fallback, and drew an empty
    // string. Now it is a name no generated function answers to.
    let defined: BTreeSet<String> = Props::accessors()
        .into_iter()
        .chain(HAND_WRITTEN.iter().map(|name| name.to_string()))
        .collect();

    let unknown: Vec<_> = called().difference(&defined).cloned().collect();
    assert!(
        unknown.is_empty(),
        "the shell calls {unknown:?}, which the node table does not generate"
    );
}

#[test]
fn every_prop_the_vocabulary_declares_is_drawn_by_something() {
    let called = called();
    let mut undrawn = Vec::new();

    for prop in SHARED {
        let accessor = Props::shared_accessor(prop);
        if !called.contains(&accessor) && !NOT_DRAWN.contains(&prop.name) {
            undrawn.push(accessor);
        }
    }

    for kind in NodeKind::ALL {
        for prop in kind.props() {
            let accessor = Props::accessor(*kind, prop);
            if !called.contains(&accessor) && !NOT_DRAWN.contains(&prop.name) {
                undrawn.push(accessor);
            }
        }
    }

    assert!(
        undrawn.is_empty(),
        "the vocabulary declares {undrawn:?} and no shell draws them. \
         Draw them, drop them, or name them in NOT_DRAWN with a reason — a \
         prop nothing reads is a unit publishing into the dark."
    );
}

#[test]
fn nothing_is_named_undrawn_and_then_drawn() {
    // The inventory is what is missing, not a place to leave a name behind.
    let called = called();

    for name in NOT_DRAWN {
        let drawn = SHARED
            .iter()
            .filter(|prop| prop.name == *name)
            .map(Props::shared_accessor)
            .chain(NodeKind::ALL.iter().flat_map(|kind| {
                kind.props()
                    .iter()
                    .filter(|prop| prop.name == *name)
                    .map(|prop| Props::accessor(*kind, prop))
            }))
            .any(|accessor| called.contains(&accessor));

        assert!(!drawn, "{name} is drawn but still listed in NOT_DRAWN");
    }
}

#[test]
fn a_prop_whose_name_two_kinds_share_gets_a_reader_each() {
    // `value` is a fraction on a slider and a string on a field. One reader
    // would have to guess, and the guess is a NaN or an empty string.
    let slider = Props::accessor(
        NodeKind::Slider,
        &NodeKind::Slider.props()[NodeKind::Slider
            .props()
            .iter()
            .position(|prop| prop.name == "value")
            .expect("a slider carries a value")],
    );
    let field = Props::accessor(
        NodeKind::Field,
        &NodeKind::Field.props()[NodeKind::Field
            .props()
            .iter()
            .position(|prop| prop.name == "value")
            .expect("a field carries a value")],
    );

    assert_ne!(slider, field);

    let generated = Props::generate();
    assert!(generated.contains(&format!("function {slider}(node)")));
    assert!(generated.contains(&format!("function {field}(node)")));
}
