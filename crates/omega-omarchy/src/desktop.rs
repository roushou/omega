//! Theme selection and installation for independent desktop presentations.
use omega_host::{Layout, StageDir};
use omega_renderer::{Build, Core, Desktop};
use std::{io, path::Path};

/// A standalone renderer with a selected host theme.
/// Omarchy's live Commons module owns theme parsing, fonts, spacing, and controls.
/// The renderer's default theme is used on machines without Omarchy. An explicit
/// `OMARCHY_PATH` must contain `shell/Commons`; an invalid override is an error.
#[derive(Debug)]
pub struct DesktopRenderer {
    theme: String,
}

impl DesktopRenderer {
    /// Resolve the installed Omarchy theme module at the host boundary.
    pub fn discover() -> io::Result<Self> {
        match std::env::var_os("OMARCHY_PATH") {
            Some(root) => Self::omarchy(&std::path::PathBuf::from(root).join("shell/Commons")),
            None => {
                let commons = Path::new("/usr/share/omarchy/shell/Commons");
                if commons.is_dir() {
                    Self::omarchy(commons)
                } else {
                    Ok(Self::default())
                }
            }
        }
    }

    /// Bind the adapter to an installed Commons directory. The directory is
    /// imported in place, so theme changes retain Omarchy's own live behavior.
    pub fn omarchy(commons: &Path) -> io::Result<Self> {
        let commons = commons.canonicalize()?;
        if !commons.join("qmldir").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Omarchy Commons has no qmldir",
            ));
        }
        let path = commons.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "QML theme import paths must be valid UTF-8",
            )
        })?;
        let mut url = String::from("file://");
        for byte in path.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
                url.push(char::from(byte));
            } else {
                url.push_str(&format!("%{byte:02X}"));
            }
        }
        let quoted = serde_json::to_string(&url).map_err(io::Error::other)?;
        let theme = include_str!("../shell/OmarchyTheme.qml")
            .replace("import QtQuick", "import QtQuick\nimport Quickshell.Io")
            .replace("import qs.Commons", &format!("import {quoted}"))
            .replace(
                "foreground: Color.foreground",
                "foreground: Color.popups.text",
            )
            .replace(
                "background: Color.background",
                "background: Color.popups.background",
            )
            .replace("Theme {", Self::BINDINGS);
        Ok(Self { theme })
    }

    const BINDINGS: &'static str = r#"Theme {
    surfacePadding: Style.spacing.popupPadding
    surfaceBorderWidth: Math.max(1, Style.space(2))
    surfaceBorderColor: Color.popups.border

    // Standalone hosts do not receive Omarchy shell's theme IPC.
    property FileView selectedTheme: FileView {
        path: Color.stateHome + "/omarchy/current/theme.name"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: {
            Color.colorsFile.reload()
            Color.shellFile.reload()
            Style.scheduleRefresh()
        }
    }
    Component.onCompleted: {
        Color.colorsFile.watchChanges = true
        Color.shellFile.watchChanges = true
    }
    property Connections paletteChanges: Connections {
        target: Color.colorsFile
        function onFileChanged() { Color.colorsFile.reload() }
    }
    property Connections styleChanges: Connections {
        target: Color.shellFile
        function onFileChanged() { Color.shellFile.reload() }
    }
"#;

    /// Identity of the complete embedded host and selected theme adapter.
    pub fn build(&self) -> Build {
        Desktop::build_with_theme(&self.theme)
    }

    /// Atomically replace the standalone renderer directory with this bundle.
    pub fn install(&self, layout: &Layout) -> io::Result<()> {
        let stage = StageDir::new(&layout.renderer_dir())?;
        let build = self.build();
        for asset in Core::FILES.iter().chain(Desktop::FILES) {
            stage.write(asset.name, build.contents(asset).as_bytes())?;
        }
        stage.write(Desktop::THEME_FILE, self.theme.as_bytes())?;
        stage.commit()
    }
}

impl Default for DesktopRenderer {
    fn default() -> Self {
        Self {
            theme: Desktop::THEME.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_bundle_uses_and_fingerprints_the_selected_theme() {
        let root = tempfile::tempdir().unwrap();
        let commons = root.path().join("Commons with spaces");
        std::fs::create_dir(&commons).unwrap();
        std::fs::write(commons.join("qmldir"), "").unwrap();
        let themed = DesktopRenderer::omarchy(&commons).unwrap();
        assert_ne!(themed.build().fingerprint(), Desktop::build().fingerprint());
        assert!(themed.theme.contains("foreground: Color.popups.text"));
        assert!(themed.theme.contains("font: Style.font"));
        assert!(!themed.theme.contains("import qs.Commons"));
        assert!(themed.theme.contains("Commons%20with%20spaces"));
        let layout = Layout::at(
            root.path().join("config"),
            root.path().join("state"),
            root.path().join("cache"),
        );
        themed.install(&layout).unwrap();
        assert_eq!(
            std::fs::read_to_string(layout.renderer_dir().join(Desktop::THEME_FILE)).unwrap(),
            themed.theme
        );
        let connection =
            std::fs::read_to_string(layout.renderer_dir().join("core/RendererConnection.qml"))
                .unwrap();
        assert!(connection.contains(themed.build().fingerprint()));
        DesktopRenderer::default().install(&layout).unwrap();
        assert_eq!(
            std::fs::read_to_string(layout.renderer_dir().join(Desktop::THEME_FILE)).unwrap(),
            Desktop::THEME
        );
    }

    #[test]
    fn missing_host_module_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        assert!(DesktopRenderer::omarchy(root.path()).is_err());
        assert_eq!(
            DesktopRenderer::default().build().fingerprint(),
            Desktop::build().fingerprint()
        );
    }
}
