use super::Glyphs;
use crate::initialize::InitFailure;
use miette::Diagnostic;
use miette::{GraphicalReportHandler, GraphicalTheme, LabeledSpan, MietteDiagnostic, NamedSource};
use omega_document::ValidationError;
use omega_omarchy::shell::ShellError;

pub(super) struct CliDiagnostic;

impl CliDiagnostic {
    pub(super) fn render(error: &anyhow::Error, glyphs: Glyphs) -> Option<String> {
        for cause in error.chain() {
            if let Some(failure) = cause.downcast_ref::<InitFailure>() {
                let diagnostic = InitializationDiagnostic {
                    failure,
                    cause: Self::source_report(&failure.source),
                };
                return Self::render_at(error, cause, &diagnostic, glyphs);
            }
            if let Some(report) = Self::report(cause) {
                return Self::render_at(error, cause, report.as_ref(), glyphs);
            }
        }
        None
    }

    fn render_at(
        error: &anyhow::Error,
        cause: &(dyn std::error::Error + 'static),
        diagnostic: &dyn Diagnostic,
        glyphs: Glyphs,
    ) -> Option<String> {
        let theme = match glyphs {
            Glyphs::Unicode => GraphicalTheme::unicode_nocolor(),
            Glyphs::Ascii => GraphicalTheme::ascii(),
        };
        let mut output = String::new();
        for context in error.chain() {
            if std::ptr::eq(context, cause) {
                break;
            }
            output.push_str(&context.to_string());
            output.push('\n');
        }
        GraphicalReportHandler::new_themed(theme)
            .render_report(&mut output, diagnostic)
            .ok()?;
        Some(output)
    }

    fn source_report(error: &anyhow::Error) -> Option<miette::Report> {
        let mut contexts = Vec::new();
        for cause in error.chain() {
            if let Some(mut report) = Self::report(cause) {
                for context in contexts.into_iter().rev() {
                    report = report.wrap_err(context);
                }
                return Some(report);
            }
            contexts.push(cause.to_string());
        }
        None
    }

    fn report(cause: &(dyn std::error::Error + 'static)) -> Option<miette::Report> {
        if let Some(shell) = cause.downcast_ref::<ShellError>() {
            Self::shell(shell)
        } else if let Some(validation) = cause.downcast_ref::<ValidationError>() {
            Self::validation(validation)
        } else if let Some(document) = cause.downcast_ref::<omega_document::Error>() {
            match document {
                omega_document::Error::Validation(validation) => Self::validation(validation),
                _ => None,
            }
        } else {
            None
        }
    }

    fn shell(error: &ShellError) -> Option<miette::Report> {
        let diagnostic = MietteDiagnostic::new(error.to_string());
        match error {
            ShellError::Parse { input, source } => {
                let offset = Self::offset(input, source.line(), source.column());
                let length = input[offset..].chars().next().map_or(0, char::len_utf8);
                Some(miette::Report::new(diagnostic
                    .with_code("omega::shell::json")
                    .with_label(LabeledSpan::at(offset..offset + length, "invalid JSON"))
                    .with_help("Correct the JSON before running omega shell adopt again."))
                    .with_source_code(NamedSource::new("shell.json", input.clone())))
            }
            ShellError::DuplicatePlacement { first, second, .. } => Some(miette::Report::new(
                diagnostic.with_code("omega::shell::duplicate_placement").with_help(format!(
                    "Choose distinct placement IDs at {first} and {second}. A plugin can appear more than once, but each placement needs its own ID."
                )),
            )),
            _ => None,
        }
    }

    fn validation(error: &ValidationError) -> Option<miette::Report> {
        match error {
            ValidationError::MissingSurface { available, .. }
            | ValidationError::AmbiguousSurface { available, .. } => {
                let help = if available.is_empty() {
                    "This plugin declares no widgets. Choose a plugin with a Surface surface."
                        .into()
                } else {
                    format!(
                        "Choose a widget surface from: {}.",
                        available
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                Some(miette::Report::new(
                    MietteDiagnostic::new(error.to_string())
                        .with_code("omega::document::widget_surface")
                        .with_help(help),
                ))
            }
            _ => None,
        }
    }

    fn offset(input: &str, line: usize, column: usize) -> usize {
        let start: usize = input
            .split_inclusive('\n')
            .take(line.saturating_sub(1))
            .map(str::len)
            .sum();
        let mut offset = (start + column.saturating_sub(1)).min(input.len());
        while !input.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }
}

/// Preserve source labels and operation context inside the initialization diagnostic.
#[derive(Debug)]
struct InitializationDiagnostic<'a> {
    failure: &'a InitFailure,
    cause: Option<miette::Report>,
}

impl std::fmt::Display for InitializationDiagnostic<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.failure.fmt(formatter)
    }
}

impl std::error::Error for InitializationDiagnostic<'_> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.failure.source.as_ref())
    }
}

impl Diagnostic for InitializationDiagnostic<'_> {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.failure.code()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.failure.help()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.cause
            .as_ref()
            .map(|cause| cause.as_ref() as &dyn Diagnostic)
    }
}
