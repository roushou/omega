use omega_proto::omega::{CommandType, Value, command_type::Kind, value};
use omega_proto::{FromValue, IntoValue};
use prost::Message;

#[test]
fn unsigned_values_round_trip_the_entire_range_without_signed_wrapping() {
    for number in [
        0,
        1,
        (1 << 53) + 1,
        i64::MAX as u64,
        i64::MAX as u64 + 1,
        u64::MAX,
    ] {
        let value = number.into_value();
        assert_eq!(value.kind, Some(value::Kind::UintValue(number)));
        let binary = Value::decode(value.encode_to_vec().as_slice()).unwrap();
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, format!(r#"{{"uintValue":"{number}"}}"#));
        let decoded: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, binary);
        assert_eq!(u64::from_value(&decoded), Some(number));
        assert!(i64::from_value(&decoded).is_none());
        assert!(CommandType::of(Kind::Unsigned).accepts(&decoded).is_ok());
        assert!(CommandType::of(Kind::Integer).accepts(&decoded).is_err());
    }
    assert_eq!(
        u64::from_value(&i64::MAX.into_value()),
        Some(i64::MAX as u64)
    );
    assert_eq!(u64::from_value(&(-1_i64).into_value()), None);
    assert!(
        CommandType::of(Kind::Unsigned)
            .accepts(&(-1_i64).into_value())
            .is_err()
    );
    assert!(
        CommandType::of(Kind::Unsigned)
            .accepts(&7_i64.into_value())
            .is_ok()
    );
}

#[test]
fn integer_bounds_do_not_round_large_values_through_doubles() {
    let boundary = 1_u64 << 53;
    for (kind, edge, beyond) in [
        (
            Kind::Unsigned,
            boundary.into_value(),
            (boundary + 1).into_value(),
        ),
        (
            Kind::Integer,
            (boundary as i64).into_value(),
            (boundary as i64 + 1).into_value(),
        ),
    ] {
        let shape = CommandType {
            maximum: Some(boundary as f64),
            ..CommandType::of(kind)
        };
        assert!(shape.accepts(&edge).is_ok());
        assert!(shape.accepts(&beyond).is_err());
    }
    let negative = CommandType {
        minimum: Some(-(boundary as f64)),
        ..CommandType::of(Kind::Integer)
    };
    assert!(negative.accepts(&(-(boundary as i64)).into_value()).is_ok());
    assert!(
        negative
            .accepts(&(-(boundary as i64) - 1).into_value())
            .is_err()
    );
    let fractional = CommandType {
        minimum: Some(0.5),
        maximum: Some(1.5),
        ..CommandType::of(Kind::Unsigned)
    };
    assert!(fractional.accepts(&0_u64.into_value()).is_err());
    assert!(fractional.accepts(&1_u64.into_value()).is_ok());
    assert!(fractional.accepts(&2_u64.into_value()).is_err());
}

#[test]
fn generic_value_presence_and_strict_json_evolution_are_explicit() {
    assert_eq!(serde_json::to_string(&Value::default()).unwrap(), "{}");
    assert_eq!(
        serde_json::to_string(&0_i64.into_value()).unwrap(),
        r#"{"intValue":"0"}"#
    );
    assert_eq!(
        serde_json::to_string(&false.into_value()).unwrap(),
        r#"{"boolValue":false}"#
    );
    assert_eq!(
        serde_json::from_str::<Value>(r#"{"stringValue":null}"#).unwrap(),
        Value::default()
    );
    for json in [
        r#"{"futureValue":1}"#,
        r#"{"intValue":"1","uintValue":"1"}"#,
        r#"{"uintValue":"-1"}"#,
        r#"{"uintValue":"18446744073709551616"}"#,
    ] {
        assert!(serde_json::from_str::<Value>(json).is_err(), "{json}");
    }
    // Prost discards unknown binary fields; an unknown oneof cannot be recovered.
    assert_eq!(Value::decode(&[0x58, 1][..]).unwrap(), Value::default());
    let bytes = Value {
        kind: Some(value::Kind::BytesValue(vec![0, 255])),
    };
    assert_eq!(
        serde_json::to_string(&bytes).unwrap(),
        r#"{"bytesValue":"AP8="}"#
    );
    assert!(omega_proto::json::Json::encode(&f64::INFINITY.into_value()).is_err());
    assert!(
        CommandType::of(Kind::Number)
            .accepts(&f64::INFINITY.into_value())
            .is_err()
    );
}

#[test]
fn json_rejects_nonfinite_numbers_at_any_depth_in_both_directions() {
    use omega_proto::{Values, json::Json};
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let nested = Values::new()
            .with("samples", vec![1.0, number])
            .into_value();
        assert!(Json::encode(&nested).is_err());
    }
    let color = Value {
        kind: Some(value::Kind::Color(omega_proto::omega::Color {
            r: f32::INFINITY,
            ..Default::default()
        })),
    };
    assert!(Json::encode(&color).is_err());
    for number in ["NaN", "Infinity", "-Infinity"] {
        let json = format!(r#"{{"list":{{"values":[{{"doubleValue":"{number}"}}]}}}}"#);
        assert!(Json::decode::<Value>(&json).is_err());
        let text = number.into_value();
        assert_eq!(
            Json::decode::<Value>(&Json::encode(&text).unwrap()).unwrap(),
            text
        );
    }
    let finite = Values::new()
        .with("samples", vec![-0.0, f64::MIN, f64::MAX])
        .into_value();
    assert_eq!(
        Json::decode::<Value>(&Json::encode(&finite).unwrap()).unwrap(),
        finite
    );
}
