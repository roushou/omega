use super::UnitName;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// A literal executable and arguments for `ExecStart`, without shell expansion.
/// Paths must be absolute and UTF-8. NUL is rejected. Rendering quotes arguments
/// with systemd environment expansion disabled and percent specifiers escaped.
#[derive(Debug, Clone)]
pub struct ExecStart {
    program: PathBuf,
    arguments: Vec<String>,
}

impl ExecStart {
    pub fn new(program: impl Into<PathBuf>) -> Result<Self, UnitError> {
        let program = program.into();
        if !program.is_absolute() {
            return Err(UnitError::Executable(program));
        }
        let text = program
            .to_str()
            .ok_or_else(|| UnitError::Executable(program.clone()))?;
        Self::word(text)?;
        Ok(Self {
            program,
            arguments: Vec::new(),
        })
    }

    pub fn arg(mut self, argument: impl Into<String>) -> Result<Self, UnitError> {
        let argument = argument.into();
        Self::word(&argument)?;
        self.arguments.push(argument);
        Ok(self)
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    fn word(text: &str) -> Result<String, UnitError> {
        if text.contains('\0') {
            return Err(UnitError::Nul);
        }
        let mut escaped = String::from("\"");
        for character in text.chars() {
            match character {
                '"' => escaped.push_str("\\\""),
                '\\' => escaped.push_str("\\\\"),
                '%' => escaped.push_str("%%"),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                c if c.is_ascii_control() => escaped.push_str(&format!("\\x{:02x}", c as u8)),
                c => escaped.push(c),
            }
        }
        escaped.push('"');
        Ok(escaped)
    }
}

impl std::fmt::Display for ExecStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            &Self::word(&format!(
                ":{}",
                self.program.to_str().expect("validated executable")
            ))
            .expect("validated executable"),
        )?;
        for argument in &self.arguments {
            write!(f, " {}", Self::word(argument).expect("validated argument"))?;
        }
        Ok(())
    }
}

/// Restart behavior for a generated service.
#[derive(Debug, Clone, Copy, Default)]
pub enum Restart {
    #[default]
    No,
    OnFailure,
    Always,
}

impl Restart {
    fn as_str(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::OnFailure => "on-failure",
            Self::Always => "always",
        }
    }
}

/// A generated `Type=simple` service unit. Defaults to no automatic restart,
/// no session dependencies, and systemd's default timeouts. Construction and
/// rendering are pure; installation and manager operations are separate.
///
/// ```
/// use omega_host::systemd::{ExecStart, ServiceUnit, UnitName};
/// let command = ExecStart::new("/opt/example/bin/worker")?.arg("serve")?;
/// let unit = ServiceUnit::new(command)
///     .description("Example worker")?
///     .wanted_by("default.target".parse::<UnitName>()?);
/// assert!(unit.to_string().contains("Type=simple"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone)]
pub struct ServiceUnit {
    command: ExecStart,
    description: Option<String>,
    documentation: Option<String>,
    after: Vec<UnitName>,
    part_of: Vec<UnitName>,
    wanted_by: Vec<UnitName>,
    restart: Restart,
    restart_delay: Option<Duration>,
    stop_timeout: Option<Duration>,
}

impl ServiceUnit {
    pub fn new(command: ExecStart) -> Self {
        Self {
            command,
            description: None,
            documentation: None,
            after: Vec::new(),
            part_of: Vec::new(),
            wanted_by: Vec::new(),
            restart: Restart::No,
            restart_delay: None,
            stop_timeout: None,
        }
    }

    pub fn description(mut self, text: &str) -> Result<Self, UnitError> {
        self.description = Some(Self::text(text)?);
        Ok(self)
    }

    pub fn documentation(mut self, url: &str) -> Result<Self, UnitError> {
        self.documentation = Some(Self::text(url)?);
        Ok(self)
    }

    fn text(text: &str) -> Result<String, UnitError> {
        if text.chars().any(char::is_control) {
            return Err(UnitError::Text);
        }
        Ok(text.replace('\\', "\\\\").replace('%', "%%"))
    }

    pub fn after(mut self, unit: UnitName) -> Self {
        self.after.push(unit);
        self
    }

    pub fn part_of(mut self, unit: UnitName) -> Self {
        self.part_of.push(unit);
        self
    }

    pub fn wanted_by(mut self, unit: UnitName) -> Self {
        self.wanted_by.push(unit);
        self
    }

    /// Select restart policy and delay, rendered at microsecond precision.
    pub fn restart(mut self, policy: Restart, delay: Duration) -> Self {
        self.restart = policy;
        self.restart_delay = Some(delay);
        self
    }

    /// Set the stop deadline, rendered at microsecond precision.
    pub fn stop_timeout(mut self, timeout: Duration) -> Self {
        self.stop_timeout = Some(timeout);
        self
    }

    pub fn command(&self) -> &ExecStart {
        &self.command
    }

    /// Return one unambiguous literal `ExecStart` declaration for display only.
    /// This does not resolve drop-ins, specifiers, or the manager's loaded command.
    /// Multiple commands and continued lines are deliberately left uninterpreted.
    pub(super) fn declared_command(source: &str) -> Option<String> {
        if source.lines().any(|line| line.trim_end().ends_with('\\')) {
            return None;
        }

        let mut service = false;
        let mut command = None;
        let mut multiple = false;
        for line in source.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                service = line == "[Service]";
                continue;
            }
            if !service {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() != "ExecStart" {
                    continue;
                }
                let value = value.trim();
                if value.is_empty() {
                    command = None;
                    multiple = false;
                } else {
                    if command.is_some() {
                        multiple = true;
                    }
                    command = Some(value.to_owned());
                }
            }
        }
        if multiple { None } else { command }
    }
}

impl std::fmt::Display for ServiceUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "[Unit]")?;
        if let Some(description) = &self.description {
            writeln!(f, "Description={description}")?;
        }
        if let Some(documentation) = &self.documentation {
            writeln!(f, "Documentation={documentation}")?;
        }
        for unit in &self.after {
            writeln!(f, "After={unit}")?;
        }
        for unit in &self.part_of {
            writeln!(f, "PartOf={unit}")?;
        }
        writeln!(
            f,
            "\n[Service]\nType=simple\nExecStart={}\nRestart={}",
            self.command,
            self.restart.as_str()
        )?;
        if let Some(delay) = self.restart_delay {
            writeln!(f, "RestartSec={}us", delay.as_micros())?;
        }
        if let Some(timeout) = self.stop_timeout {
            writeln!(f, "TimeoutStopSec={}us", timeout.as_micros())?;
        }
        if !self.wanted_by.is_empty() {
            writeln!(f, "\n[Install]")?;
            for unit in &self.wanted_by {
                writeln!(f, "WantedBy={unit}")?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UnitError {
    #[error("systemd executable must be an absolute UTF-8 path: {}", .0.display())]
    Executable(PathBuf),
    #[error("systemd command words cannot contain NUL")]
    Nul,
    #[error("systemd unit text must be a single line without control characters")]
    Text,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_words_escape_systemd_syntax_without_shell_interpolation() {
        let command = ExecStart::new("/opt/a b/%user/$bin/omega")
            .unwrap()
            .arg("a\"b\\c\n")
            .unwrap()
            .arg("")
            .unwrap();
        assert_eq!(
            command.to_string(),
            "\":/opt/a b/%%user/$bin/omega\" \"a\\\"b\\\\c\\n\" \"\""
        );
        assert!(ExecStart::new("relative/bin").is_err());
        assert!(ExecStart::new("/bin/app").unwrap().arg("a\0b").is_err());
    }

    #[test]
    fn ambiguous_or_reset_commands_are_not_reported_as_a_known_executable() {
        assert_eq!(
            ServiceUnit::declared_command(
                r"[Service]
Description=continued \
ExecStart=/not-a-command
"
            ),
            None
        );
        assert_eq!(
            ServiceUnit::declared_command(
                "[Unit]\nExecStart=/wrong\n[Service]\nExecStart=\"/a b/worker\" serve\n"
            ),
            Some("\"/a b/worker\" serve".into())
        );
        assert_eq!(
            ServiceUnit::declared_command("[Service]\nExecStart=/a\nExecStart=/b\n"),
            None
        );
        assert_eq!(
            ServiceUnit::declared_command("[Service]\nExecStart=/a\nExecStart=\nExecStart=/b\n"),
            Some("/b".into())
        );
    }
}
