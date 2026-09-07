//! The manifest is the daemon's, not the unit's: identity and grants are read
//! from the copy on disk and a peer's claim is only ever checked against it.

mod common;

use std::path::{Path, PathBuf};

use common::{Harness, expect_refusal, unit_name, widget_manifest};
use omega_daemon::host::StateConfig;
use omega_daemon::manifest::ManifestStore;
use omega_proto::Layout;
use omega_proto::Manifest;
use omega_proto::omega::{Capability, ErrorCode, frame};

struct StateDir(PathBuf);

impl StateDir {
    /// A state dir holding one built unit and its canonical manifest, the
    /// shape `omega build` leaves behind.
    fn with_unit(tag: &str, manifest: &Manifest) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "omega-manifest-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let state = Self(dir);
        state
            .layout()
            .file::<Manifest>(&manifest.name)
            .write(manifest)
            .unwrap();
        state
    }

    fn layout(&self) -> Layout {
        Layout::at(self.0.clone(), self.0.clone(), self.0.clone())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for StateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn store(state: &StateDir, manifest: &Manifest) -> ManifestStore {
    let layout = state.layout();
    let config = StateConfig::new(&layout, [manifest.name.clone()]);
    ManifestStore::load(&config, &layout).unwrap()
}

#[tokio::test]
async fn a_manifest_that_lies_about_its_unit_fails_the_load() {
    let manifest = widget_manifest("battery-widget", "battery");
    let state = StateDir::with_unit("mismatch", &manifest);

    // Same file, claimed for a different unit: the store refuses to vouch.
    let layout = state.layout();
    let config = StateConfig::new(&layout, [unit_name("clock")]);
    let err = ManifestStore::load(&config, &layout).unwrap_err();

    assert!(err.to_string().contains("clock"), "{err}");
    let _ = state.path();
}

#[tokio::test]
async fn a_wrong_manifest_hash_is_refused_with_a_reason() {
    let manifest = widget_manifest("battery-widget", "battery");
    let state = StateDir::with_unit("reject", &manifest);
    let harness = Harness::new("manifest-reject", store(&state, &manifest));
    let token = harness.register_unit("battery-widget");

    let mut transport = harness.connect("wrong-hash", token.as_str()).await;

    let refusal = expect_refusal(transport.recv().await.unwrap());
    assert_eq!(refusal.code, ErrorCode::FailedPrecondition);
    assert!(refusal.message.contains("hash mismatch"), "{refusal}");
    assert!(transport.recv().await.unwrap().is_none(), "expected EOF");
}

#[tokio::test]
async fn a_matching_hash_is_granted_exactly_what_the_manifest_declares() {
    let manifest = widget_manifest("battery-widget", "battery");
    let state = StateDir::with_unit("accept", &manifest);
    let harness = Harness::new("manifest-accept", store(&state, &manifest));
    let token = harness.register_unit("battery-widget");

    let mut transport = harness.connect(&manifest.hash(), token.as_str()).await;

    match transport.recv().await.unwrap().unwrap().body {
        Some(frame::Body::Welcome(w)) => {
            assert_eq!(w.unit_id, "battery-widget");
            assert_eq!(w.capabilities, vec![Capability::StateRead as i32]);
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
}
