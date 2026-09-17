//! Open a declared surface in an independent presentation.
use crate::{
    operator::Operator,
    ui::{Paint, Step, Ui},
};
use omega_proto::instance::{PresentationError, PresentationSpec};
use omega_proto::omega::{self, presentation};
use omega_proto::{SurfaceId, UnitName};

#[derive(Debug, clap::Args)]
pub struct PresentCmd {
    #[arg(value_name = "UNIT")]
    pub unit_name: UnitName,
    #[arg(value_name = "SURFACE")]
    pub surface_id: SurfaceId,
    /// Create an independent instance instead of reusing this entry point.
    #[arg(long)]
    pub new: bool,
    /// Open as a keyboard-focused overlay.
    #[arg(long)]
    pub overlay: bool,
    /// Dismiss an overlay when clicking outside its content.
    #[arg(long, requires = "overlay")]
    pub dismiss_on_outside: bool,
    /// Present the overlay on this output; otherwise the host chooses.
    #[arg(long, requires = "overlay")]
    pub output: Option<String>,
    #[arg(long, default_value_t = 480)]
    pub width: u32,
    #[arg(long, default_value_t = 320)]
    pub height: u32,
    /// Construction settings as a JSON object, e.g. '{"label":"Example"}'.
    #[arg(long)]
    pub config: Option<String>,
    /// Print the accepted instance snapshot as JSON.
    #[arg(long)]
    pub json: bool,
}
impl PresentCmd {
    fn settings(json: &str) -> anyhow::Result<std::collections::HashMap<String, omega::Value>> {
        let fields: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(json)?;
        fields
            .into_iter()
            .map(|(key, value)| Ok((key, Self::value(value)?)))
            .collect()
    }

    fn value(value: serde_json::Value) -> anyhow::Result<omega::Value> {
        use omega::value::Kind;
        let kind = match value {
            serde_json::Value::Null => None,
            serde_json::Value::Bool(value) => Some(Kind::BoolValue(value)),
            serde_json::Value::String(value) => Some(Kind::StringValue(value)),
            serde_json::Value::Number(value) => Some(if let Some(integer) = value.as_i64() {
                Kind::IntValue(integer)
            } else if value.is_f64() {
                Kind::DoubleValue(
                    value
                        .as_f64()
                        .ok_or_else(|| anyhow::anyhow!("number is outside the supported range"))?,
                )
            } else {
                anyhow::bail!("integer is outside the supported range; encode it as text")
            }),
            serde_json::Value::Array(values) => Some(Kind::List(omega::ListValue {
                values: values
                    .into_iter()
                    .map(Self::value)
                    .collect::<Result<_, _>>()?,
            })),
            serde_json::Value::Object(values) => Some(Kind::Map(omega::MapValue {
                entries: values
                    .into_iter()
                    .map(|(key, value)| Ok((key, Self::value(value)?)))
                    .collect::<anyhow::Result<_>>()?,
            })),
        };
        Ok(omega::Value { kind })
    }

    fn presentation(&self) -> Result<PresentationSpec, PresentationError> {
        let unit_name = &self.unit_name;
        let surface_id = &self.surface_id;
        let kind = if self.overlay {
            presentation::Kind::Overlay(omega::OverlayPresentation {
                width: self.width,
                height: self.height,
                output: self.output.clone().unwrap_or_default(),
                dismiss_on_outside: self.dismiss_on_outside,
                keyboard: omega::KeyboardPolicy::Exclusive as i32,
            })
        } else {
            presentation::Kind::Window(omega::WindowPresentation {
                title: format!("{unit_name} — {surface_id}"),
                app_id: format!("org.omega.{unit_name}"),
                width: self.width,
                height: self.height,
                min_width: 1,
                min_height: 1,
            })
        };
        PresentationSpec::try_from(omega::Presentation { kind: Some(kind) })
    }

    pub async fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let presentation = self.presentation()?;
        let unit_name = self.unit_name;
        let surface_id = self.surface_id;
        let result = Operator::new()
            .present(omega::CreateInstance {
                unit: unit_name.to_string(),
                surface: surface_id.to_string(),
                presentation: Some(presentation.wire().clone()),
                config: self
                    .config
                    .map(|config| Self::settings(&config))
                    .transpose()?
                    .unwrap_or_default(),
                singleton: if self.new {
                    String::new()
                } else {
                    "default".into()
                },
            })
            .await?;
        if self.json {
            ui.line(serde_json::to_string(&result)?);
        } else {
            ui.step(
                Step::Requested,
                Paint::name(format!("{unit_name}.{surface_id}")),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use clap::Parser;

    struct Fixture;

    impl Fixture {
        fn command(overlay: bool, dimension: &str, size: u32) -> PresentCmd {
            let size = size.to_string();
            let mut args = vec!["omega", "present", "audio", "panel", dimension, &size];
            if overlay {
                args.push("--overlay");
            }
            let Command::Present(command) = Cli::try_parse_from(args).unwrap().command else {
                panic!("expected present")
            };
            command
        }
    }

    #[tokio::test]
    async fn invalid_dimensions_fail_before_contacting_the_daemon() {
        for overlay in [false, true] {
            for dimension in ["--width", "--height"] {
                for size in [0, 16385, u32::MAX] {
                    let command = Fixture::command(overlay, dimension, size);
                    let (mut ui, _) = Ui::recording();
                    let error = command.run(&mut ui).await.unwrap_err();
                    assert!(matches!(
                        error.downcast_ref::<PresentationError>(),
                        Some(PresentationError::Size)
                    ));
                }
            }
        }
    }

    #[test]
    fn presentation_dimension_boundaries_are_accepted() {
        for overlay in [false, true] {
            for dimension in ["--width", "--height"] {
                for size in [1, 16384] {
                    Fixture::command(overlay, dimension, size)
                        .presentation()
                        .unwrap();
                }
            }
        }
    }
}
