//! Recovery commands against isolated state, through the real CLI.
use omega_document::{Document, DocumentFile};
use omega_host::{Generations, Layout, StateConfig, TempPath};
use std::path::PathBuf;
use std::process::{Command, Output};

struct Machine {
    root: PathBuf,
    layout: Layout,
}
impl Machine {
    fn new() -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-recovery-cli"), "test");
        let layout = Layout::at(root.join("config"), root.join("state"), root.join("cache"));
        Self { root, layout }
    }
    fn publish(&self, valid: bool) -> omega_host::Generation {
        let store = Generations::new(&self.layout);
        let stage = store.stage().unwrap();
        stage
            .files()
            .file::<StateConfig>(StateConfig::FILE_NAME)
            .write(&StateConfig::new(&self.layout, []))
            .unwrap();
        let document = if valid {
            DocumentFile::encode(&Document::new().into_inner()).unwrap()
        } else {
            "{ invalid".into()
        };
        stage
            .files()
            .write(DocumentFile::FILE_NAME, document.as_bytes())
            .unwrap();
        stage.commit().unwrap();
        store.pin_current().unwrap().unwrap()
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_omega"))
            .args(args)
            .env("OMEGA_CONFIG_DIR", &self.layout.config)
            .env("OMEGA_STATE_DIR", &self.layout.state)
            .env("OMEGA_CACHE_DIR", &self.layout.cache)
            .output()
            .unwrap()
    }
}
impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn rollback_validates_before_publication_and_clean_preserves_recovery() {
    let machine = Machine::new();
    let store = Generations::new(&machine.layout);
    let first = machine.publish(true);
    first.accept().unwrap();
    let second = machine.publish(true);
    second.accept().unwrap();
    let output = machine.run(&["rollback"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(store.pin_current().unwrap().unwrap().id(), first.id());

    let invalid = machine.publish(false);
    let output = machine.run(&["rollback"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(store.pin_current().unwrap().unwrap().id(), second.id());
    let output = machine.run(&["rollback", invalid.id().as_str()]);
    assert!(!output.status.success());
    assert_eq!(store.pin_current().unwrap().unwrap().id(), second.id());

    let invalid_path = invalid.layout().state.clone();
    drop(invalid);
    let output = machine.run(&["clean", "--generations"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!invalid_path.exists());
    assert!(first.layout().state.exists());
    assert!(second.layout().state.exists());
}
