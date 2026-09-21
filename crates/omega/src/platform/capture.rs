//! Screen capture: screenshots, OCR, and screen recording.

use crate::effect::Effect;
use crate::runtime::context::Context;
use crate::wiring::does;

use omega_proto::omega::{
    CaptureText, RecordScreen, RecordingConfig, Screenshot, action, record_screen,
};

/// Permission to take screenshots through grim and slurp.
///
/// ```no_run
/// use omega::{Command, platform::capture::Capture};
/// #[derive(omega::Command)]
/// struct Snapshot { capture: Capture }
/// impl Command for Snapshot {
///     const ID: &'static str = "snapshot";
///
///     type Input = ();
///     type Output = ();
///     async fn call(&self, _: ()) -> omega::Result<()> {
///         self.capture.clipboard().await
///     }
/// }
/// ```
#[derive(Debug)]
pub struct Capture {
    context: Context,
}

does!(Capture, Screenshot);

impl Capture {
    /// Capture the whole screen to a file.
    pub fn fullscreen(&self, path: impl Into<String>) -> Effect {
        self.screenshot(Screenshot {
            fullscreen: true,
            output_path: path.into(),
            ..Default::default()
        })
    }

    /// Select a region and save it to a file.
    pub fn region(&self, path: impl Into<String>) -> Effect {
        self.screenshot(Screenshot {
            output_path: path.into(),
            ..Default::default()
        })
    }

    /// Capture one monitor's full screen to a file.
    pub fn monitor(&self, monitor: &str, path: impl Into<String>) -> Effect {
        self.screenshot(Screenshot {
            fullscreen: true,
            region_monitor_id: monitor.into(),
            output_path: path.into(),
            ..Default::default()
        })
    }

    /// Select a region and copy it to the clipboard.
    pub fn clipboard(&self) -> Effect {
        self.screenshot(Screenshot {
            clipboard: true,
            ..Default::default()
        })
    }

    fn screenshot(&self, shot: Screenshot) -> Effect {
        self.act(action::Kind::Screenshot(shot))
    }
}

/// Permission to recognize text from the screen. The recognized text is placed
/// on the clipboard, matching the desktop's own text-capture behavior.
#[derive(Debug)]
pub struct Ocr {
    context: Context,
}

does!(Ocr, Screenshot);

impl Ocr {
    /// Select a region and copy its recognized text to the clipboard.
    pub fn region(&self) -> Effect {
        self.act(action::Kind::CaptureText(CaptureText::default()))
    }

    /// Recognize the given monitor's full screen into the clipboard.
    pub fn monitor(&self, monitor: &str) -> Effect {
        self.act(action::Kind::CaptureText(CaptureText {
            region_monitor_id: monitor.into(),
        }))
    }
}

/// Permission to start and stop a screen recording. Completion confirms the
/// recording started or stopped, not that the file is complete or valid.
#[derive(Debug)]
pub struct Recording {
    context: Context,
}

does!(Recording, Screenshot);

impl Recording {
    /// Record the whole screen to a file.
    pub fn fullscreen(&self, path: impl Into<String>) -> Effect {
        self.start(RecordingConfig {
            fullscreen: true,
            output_path: path.into(),
            ..Default::default()
        })
    }

    /// Select a region and record it to a file.
    pub fn region(&self, path: impl Into<String>) -> Effect {
        self.start(RecordingConfig {
            output_path: path.into(),
            ..Default::default()
        })
    }

    /// Start a recording with explicit options, such as desktop audio.
    pub fn start(&self, config: RecordingConfig) -> Effect {
        self.act(action::Kind::RecordScreen(RecordScreen {
            command: Some(record_screen::Command::Start(config)),
        }))
    }

    /// Stop the active recording. Stopping with no recording running is an
    /// effect error, not a silent success.
    pub fn stop(&self) -> Effect {
        self.act(action::Kind::RecordScreen(RecordScreen {
            command: Some(record_screen::Command::Stop(true)),
        }))
    }
}
