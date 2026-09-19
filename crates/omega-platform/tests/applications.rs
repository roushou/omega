use omega_platform::{Applications, Broker};
use omega_proto::omega::{LaunchApp, action, state_topic};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("omega-applications-{}", std::process::id()));
        std::fs::create_dir_all(root.join("user/applications")).unwrap();
        std::fs::create_dir_all(root.join("system/applications")).unwrap();
        Self(root)
    }
    fn entry(root: &Path, base: &str, name: &str, fields: &str) {
        std::fs::write(
            root.join(base).join("applications").join(name),
            format!("[Desktop Entry]\nType=Application\nName=Fixture\nExec=/bin/true\n{fields}\n"),
        )
        .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn isolated_desktop_entry_contract() {
    let fixture = Fixture::new();
    Fixture::entry(&fixture.0, "system", "override.desktop", "");
    Fixture::entry(&fixture.0, "user", "override.desktop", "Hidden=true");
    Fixture::entry(&fixture.0, "user", "nodisplay.desktop", "NoDisplay=true");
    Fixture::entry(
        &fixture.0,
        "user",
        "elsewhere.desktop",
        "OnlyShowIn=OtherDesktop;",
    );
    Fixture::entry(
        &fixture.0,
        "user",
        "missing.desktop",
        "TryExec=/omega-fixture-missing",
    );
    Fixture::entry(
        &fixture.0,
        "user",
        "localized.desktop",
        "Name[fr]=Fichiers\nOnlyShowIn=Omega;\nKeywords[fr]=dossiers;\nIcon=system-file-manager",
    );
    let script = fixture.0.join("record arguments");
    std::fs::write(&script, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$OMEGA_APPLICATION_TEST/arguments\"\nprintf '%s' \"$XDG_ACTIVATION_TOKEN\" > \"$OMEGA_APPLICATION_TEST/token\"\nsleep 2\ntouch \"$OMEGA_APPLICATION_TEST/exited\"\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        fixture.0.join("user/applications/with spaces.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=With Spaces\nExec=\"{}\" %U\n",
            script.display()
        ),
    )
    .unwrap();
    std::fs::create_dir_all(fixture.0.join("bin")).unwrap();
    for terminal in ["xdg-terminal-exec", "gnome-terminal", "xterm"] {
        let path = fixture.0.join("bin").join(terminal);
        std::fs::write(&path, "#!/bin/sh\ntouch \"$OMEGA_APPLICATION_TEST/terminal\"\nwhile [ \"${1#-}\" != \"$1\" ]; do shift; done\nexec \"$@\"\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    Fixture::entry(&fixture.0, "user", "terminal.desktop", "Terminal=true");
    Fixture::entry(
        &fixture.0,
        "user",
        "org.omega.Fixture.desktop",
        "DBusActivatable=true",
    );
    let output = std::process::Command::new("dbus-run-session")
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "desktop_entry_child", "--nocapture"])
        .env("OMEGA_APPLICATION_TEST", &fixture.0)
        .env(
            "PATH",
            format!(
                "{}:{}",
                fixture.0.join("bin").display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("XDG_DATA_HOME", fixture.0.join("user"))
        .env("XDG_DATA_DIRS", fixture.0.join("system"))
        .env("XDG_CURRENT_DESKTOP", "Omega")
        .env("LANGUAGE", "fr")
        .env("DESKTOP_STARTUP_ID", "stale-daemon-token")
        .env("XDG_ACTIVATION_TOKEN", "stale-daemon-token")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn desktop_entry_child() {
    let Some(root) = std::env::var_os("OMEGA_APPLICATION_TEST") else {
        return;
    };
    let root = PathBuf::from(root);
    let mut broker = Applications::new();
    broker.connect().await.unwrap();
    let patch = broker.read().await.unwrap();
    let Some(state_topic::Value::Applications(apps)) = &patch.topics[0].value else {
        panic!("catalogue");
    };
    assert_eq!(apps.applications.len(), 4, "{apps:?}");
    let localized = apps
        .applications
        .iter()
        .find(|app| app.id == "localized.desktop")
        .unwrap();
    assert_eq!(localized.name, "Fichiers");
    assert_eq!(localized.keywords, ["dossiers"]);
    assert_eq!(localized.icon, "system-file-manager");
    let launch = LaunchApp {
        desktop_id: "with spaces.desktop".into(),
        uris: vec![
            "https://example.invalid/a%20b".into(),
            "https://example.invalid/$(touch injected);literal".into(),
        ],
        activation_token: String::new(),
    };
    tokio::time::timeout(
        Duration::from_secs(1),
        broker.act(&action::Kind::LaunchApp(launch.clone())),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        !root.join("exited").exists(),
        "launch waited for child exit"
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while !root.join("token").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("arguments")).unwrap(),
        format!("{}\n", launch.uris.join("\n"))
    );
    assert_eq!(std::fs::read_to_string(root.join("token")).unwrap(), "");
    assert!(
        broker
            .act(&action::Kind::LaunchApp(LaunchApp {
                desktop_id: "gone.desktop".into(),
                ..Default::default()
            }))
            .await
            .is_err()
    );
    Fixture::entry(&root, "user", "added.desktop", "");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            broker.wake().await.unwrap();
            let patch = broker.read().await.unwrap();
            let Some(state_topic::Value::Applications(apps)) = &patch.topics[0].value else {
                panic!("catalogue");
            };
            if apps
                .applications
                .iter()
                .any(|app| app.id == "added.desktop")
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    broker
        .act(&action::Kind::LaunchApp(LaunchApp {
            desktop_id: "terminal.desktop".into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !root.join("terminal").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let _service = zbus::connection::Builder::session()
        .unwrap()
        .name("org.omega.Fixture")
        .unwrap()
        .serve_at("/org/omega/Fixture", TestApplication(calls.clone()))
        .unwrap()
        .build()
        .await
        .unwrap();
    let app = action::Kind::LaunchApp(LaunchApp {
        desktop_id: "org.omega.Fixture.desktop".into(),
        ..Default::default()
    });
    broker.act(&app).await.unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(
        broker.act(&app).await.is_err(),
        "D-Bus refusal must propagate without Exec fallback"
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    // Reap the fixture's observable work before its parent removes the temporary root.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !root.join("exited").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

struct TestApplication(std::sync::Arc<std::sync::atomic::AtomicUsize>);
#[zbus::interface(name = "org.freedesktop.Application")]
impl TestApplication {
    async fn activate(
        &self,
        _platform_data: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Ok(())
        } else {
            Err(zbus::fdo::Error::Failed(
                "fixture activation refused".into(),
            ))
        }
    }
}
