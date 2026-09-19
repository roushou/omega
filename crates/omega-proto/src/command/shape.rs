use super::CommandContractError;
use crate::CommandId;
use crate::omega::{CommandEndpoint, CommandType, command_type};

impl CommandType {
    pub fn of(kind: command_type::Kind) -> Self {
        Self {
            kind: kind as i32,
            ..Self::default()
        }
    }
    pub fn canonical(&self) -> Self {
        let mut result = self.clone();
        result.fields.sort_by(|a, b| a.name.cmp(&b.name));
        for field in &mut result.fields {
            field.value = field.value.as_ref().map(Self::canonical);
        }
        result.element = result
            .element
            .as_ref()
            .map(|value| Box::new(value.canonical()));
        result.choices.sort();
        result.choices.dedup();
        result
    }
    pub fn validate(&self) -> Result<(), CommandContractError> {
        self.validate_at(0)
    }
    fn validate_at(&self, depth: usize) -> Result<(), CommandContractError> {
        let invalid =
            || CommandContractError::Invalid("malformed or excessively nested value shape".into());
        if depth > 16
            || self.fields.len() > 128
            || self.choices.len() > 256
            || self.identity.len() > 256
        {
            return Err(invalid());
        }
        let kind = command_type::Kind::try_from(self.kind).map_err(|_| invalid())?;
        use command_type::Kind;
        if self.element.is_some() != matches!(kind, Kind::List | Kind::Optional)
            || (!self.fields.is_empty() && kind != Kind::Record)
            || (!self.choices.is_empty() && kind != Kind::Choice)
            || (kind == Kind::Choice && self.choices.is_empty())
            || self.minimum.is_some_and(|v| !v.is_finite())
            || self.maximum.is_some_and(|v| !v.is_finite())
            || matches!((self.minimum,self.maximum), (Some(a),Some(b)) if a > b)
            || ((self.minimum.is_some() || self.maximum.is_some())
                && !matches!(kind, Kind::Integer | Kind::Number))
        {
            return Err(invalid());
        }
        let mut fields = std::collections::BTreeSet::new();
        for field in &self.fields {
            if field.name.is_empty() || field.name.len() > 128 || !fields.insert(&field.name) {
                return Err(invalid());
            }
            field
                .value
                .as_ref()
                .ok_or_else(invalid)?
                .validate_at(depth + 1)?;
        }
        if let Some(element) = &self.element {
            element.validate_at(depth + 1)?;
        }
        Ok(())
    }
}
impl CommandEndpoint {
    pub fn validate(&self) -> Result<CommandId, CommandContractError> {
        let id = self.id.parse()?;
        if self.description.len() > 4096 {
            return Err(CommandContractError::Invalid(
                "description exceeds 4096 bytes".into(),
            ));
        }
        self.input
            .as_ref()
            .ok_or_else(|| CommandContractError::Invalid("missing input shape".into()))?
            .validate()?;
        self.output
            .as_ref()
            .ok_or_else(|| CommandContractError::Invalid("missing output shape".into()))?
            .validate()?;
        Ok(id)
    }
}

impl CommandType {
    /// Validate a value's structural contract. The target also runs its domain decoder.
    pub fn accepts(&self, value: &crate::omega::Value) -> Result<(), CommandContractError> {
        self.validate()?;
        self.accepts_at(value, 0)
    }
    fn accepts_at(
        &self,
        value: &crate::omega::Value,
        depth: usize,
    ) -> Result<(), CommandContractError> {
        use crate::omega::{command_type::Kind, value::Kind as V};
        let invalid =
            || CommandContractError::Invalid("value does not match command signature".into());
        if depth > 16 {
            return Err(invalid());
        }
        match (
            Kind::try_from(self.kind).map_err(|_| invalid())?,
            value.kind.as_ref(),
        ) {
            (Kind::Opaque, _)
            | (Kind::Unit, None)
            | (Kind::Boolean, Some(V::BoolValue(_)))
            | (Kind::Text, Some(V::StringValue(_))) => Ok(()),
            (Kind::Integer, Some(V::IntValue(n))) => self.number(*n as f64),
            (Kind::Number, Some(V::DoubleValue(n))) => self.number(*n),
            (Kind::Choice, Some(V::StringValue(v))) if self.choices.contains(v) => Ok(()),
            (Kind::Optional, None) => Ok(()),
            (Kind::Optional, _) => self
                .element
                .as_ref()
                .ok_or_else(invalid)?
                .accepts_at(value, depth + 1),
            (Kind::List, Some(V::List(v))) => {
                let element = self.element.as_ref().ok_or_else(invalid)?;
                for item in &v.values {
                    element.accepts_at(item, depth + 1)?;
                }
                Ok(())
            }
            (Kind::Record, Some(V::Map(v))) if v.entries.len() == self.fields.len() => {
                for field in &self.fields {
                    field
                        .value
                        .as_ref()
                        .ok_or_else(invalid)?
                        .accepts_at(v.entries.get(&field.name).ok_or_else(invalid)?, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(invalid()),
        }
    }
    fn number(&self, value: f64) -> Result<(), CommandContractError> {
        if !value.is_finite()
            || self.minimum.is_some_and(|v| value < v)
            || self.maximum.is_some_and(|v| value > v)
        {
            Err(CommandContractError::Invalid(
                "numeric value outside command bounds".into(),
            ))
        } else {
            Ok(())
        }
    }
}

impl CommandEndpoint {
    /// Validate the typed argument envelope against this endpoint's input shape.
    pub fn accepts_arguments(
        &self,
        args: &[crate::omega::Value],
    ) -> Result<(), CommandContractError> {
        let input = self
            .input
            .as_ref()
            .ok_or_else(|| CommandContractError::Invalid("missing input shape".into()))?;
        match command_type::Kind::try_from(input.kind) {
            Ok(command_type::Kind::Opaque) => Ok(()),
            Ok(command_type::Kind::Unit) if args.is_empty() => Ok(()),
            Ok(command_type::Kind::Unit) => Err(CommandContractError::Invalid(
                "command expects no arguments".into(),
            )),
            Ok(_) if args.len() == 1 => input.accepts(&args[0]),
            _ => Err(CommandContractError::Invalid(
                "invalid command argument envelope".into(),
            )),
        }
    }

    /// Validate a terminal answer without conflating an empty value and an acknowledgement.
    pub fn accepts_answer(
        &self,
        answer: &super::CommandAnswer,
    ) -> Result<(), CommandContractError> {
        let output = self
            .output
            .as_ref()
            .ok_or_else(|| CommandContractError::Invalid("missing output shape".into()))?;
        match answer {
            super::CommandAnswer::Value(value) => output.accepts(value),
            super::CommandAnswer::Acknowledged
                if matches!(
                    command_type::Kind::try_from(output.kind),
                    Ok(command_type::Kind::Unit | command_type::Kind::Opaque)
                ) =>
            {
                Ok(())
            }
            _ => Err(CommandContractError::Invalid(
                "command returned no value".into(),
            )),
        }
    }
}
