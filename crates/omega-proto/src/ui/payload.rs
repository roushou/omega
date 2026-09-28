use super::DefaultValue;
use crate::omega::{Value, value};

/// The control value appended to a bound command, or delivered to a local binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    None,
    Text,
    Flag,
    Fraction,
    /// A finite fraction between zero and one.
    Percent,
    Unsigned,
    /// Canonical encoding is double; legacy integral gesture coordinates are accepted.
    Real,
    TextEdit,
    Gesture,
    FormFields,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PayloadField {
    pub name: &'static str,
    pub payload: Payload,
    pub fallback: Option<DefaultValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub name: &'static str,
    pub payload: Payload,
}

impl Payload {
    /// Stable descriptor name used by generated renderer encoders.
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Text => "text",
            Self::Flag => "flag",
            Self::Fraction => "fraction",
            Self::Percent => "percent",
            Self::Unsigned => "unsigned",
            Self::Real => "real",
            Self::TextEdit => "edit",
            Self::Gesture => "gesture",
            Self::FormFields => "form",
        }
    }

    /// Structured fields; no fallback means required. Additional fields are ignored.
    pub const fn fields(self) -> &'static [PayloadField] {
        match self {
            Self::TextEdit => &[
                PayloadField {
                    name: "text",
                    payload: Self::Text,
                    fallback: None,
                },
                PayloadField {
                    name: "revision",
                    payload: Self::Unsigned,
                    fallback: None,
                },
                PayloadField {
                    name: "reset",
                    payload: Self::Unsigned,
                    fallback: None,
                },
            ],
            Self::Gesture => &[
                PayloadField {
                    name: "zoom",
                    payload: Self::Real,
                    fallback: Some(DefaultValue::Fraction(1.0)),
                },
                PayloadField {
                    name: "offset_x",
                    payload: Self::Real,
                    fallback: Some(DefaultValue::Fraction(0.0)),
                },
                PayloadField {
                    name: "offset_y",
                    payload: Self::Real,
                    fallback: Some(DefaultValue::Fraction(0.0)),
                },
                PayloadField {
                    name: "x",
                    payload: Self::Real,
                    fallback: Some(DefaultValue::Fraction(0.0)),
                },
                PayloadField {
                    name: "y",
                    payload: Self::Real,
                    fallback: Some(DefaultValue::Fraction(0.0)),
                },
                PayloadField {
                    name: "dx",
                    payload: Self::Real,
                    fallback: Some(DefaultValue::Fraction(0.0)),
                },
                PayloadField {
                    name: "dy",
                    payload: Self::Real,
                    fallback: Some(DefaultValue::Fraction(0.0)),
                },
            ],
            _ => &[],
        }
    }

    /// Validate presence, representation, and finite numeric values.
    pub fn accepts(self, value: Option<&Value>) -> bool {
        let kind = value.and_then(|value| value.kind.as_ref());
        match (self, kind) {
            (Self::None, _) => value.is_none(),
            (Self::Text, Some(value::Kind::StringValue(_)))
            | (Self::Flag, Some(value::Kind::BoolValue(_))) => true,
            (Self::Fraction | Self::Real, Some(value::Kind::DoubleValue(value))) => {
                value.is_finite()
            }
            (Self::Percent, Some(value::Kind::DoubleValue(value))) => (0.0..=1.0).contains(value),
            (Self::Real, Some(value::Kind::IntValue(_))) => true,
            (Self::Unsigned, Some(value::Kind::IntValue(value))) => u32::try_from(*value).is_ok(),
            (Self::TextEdit | Self::Gesture, Some(value::Kind::Map(map))) => self
                .fields()
                .iter()
                .all(|field| match map.entries.get(field.name) {
                    Some(value) => field.payload.accepts(Some(value)),
                    None => field.fallback.is_some(),
                }),
            (Self::FormFields, Some(value::Kind::Map(map))) => map
                .entries
                .values()
                .all(|value| Self::Text.accepts(Some(value))),
            _ => false,
        }
    }
}
