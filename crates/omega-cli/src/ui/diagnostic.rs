use super::Glyphs;
use miette::{GraphicalReportHandler, GraphicalTheme, LabeledSpan, MietteDiagnostic, NamedSource};
use omega_document::{ValidationError, shell::ShellError};

pub(super) struct CliDiagnostic;

impl CliDiagnostic {
    pub(super) fn render(error: &anyhow::Error, glyphs: Glyphs) -> Option<String> {
        for cause in error.chain() {
            let report = if let Some(shell) = cause.downcast_ref::<ShellError>() {
                Self::shell(shell)
            } else if let Some(validation) = cause.downcast_ref::<ValidationError>() {
                Self::validation(validation)
            } else if let Some(document) = cause.downcast_ref::<omega_document::Error>() {
                match document {
                    omega_document::Error::Shell(shell) => Self::shell(shell),
                    omega_document::Error::Validation(validation) => Self::validation(validation),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(report) = report {
                let theme = match glyphs {
                    Glyphs::Unicode => GraphicalTheme::unicode_nocolor(),
                    Glyphs::Ascii => GraphicalTheme::ascii(),
                };
                let mut output = String::new();
                // Context remains above the diagnostic; the source is rendered exactly once.
                for context in error.chain() {
                    if std::ptr::eq(context, cause) {
                        break;
                    }
                    output.push_str(&context.to_string());
                    output.push('\n');
                }
                GraphicalReportHandler::new_themed(theme)
                    .render_report(&mut output, report.as_ref())
                    .ok()?;
                return Some(output);
            }
        }
        None
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
            ValidationError::Shell(shell) => Self::shell(shell),
            ValidationError::MissingSurface { available, .. }
            | ValidationError::AmbiguousSurface { available, .. } => {
                let help = if available.is_empty() {
                    "This plugin declares no widgets. Choose a plugin with a Widget surface.".into()
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
