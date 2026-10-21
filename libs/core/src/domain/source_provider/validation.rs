use std::collections::BTreeSet;

use serde_json::{Map, Number, Value};
use url::Url;

use super::descriptor::{FieldKind, FieldSpec, SourceProviderDescriptor};

const MAX_TEXT_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProblem {
    pub field: String,
    pub message: String,
}

impl FieldProblem {
    fn new(field: &str, message: impl Into<String>) -> Self {
        Self {
            field: field.to_owned(),
            message: message.into(),
        }
    }
}

fn default_value(spec: &FieldSpec) -> Option<Value> {
    let default = spec.default?;
    match spec.kind {
        FieldKind::Bool => default.parse().ok().map(Value::Bool),
        FieldKind::Number => default
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number),
        _ => Some(Value::String(default.to_owned())),
    }
}

fn check(spec: &FieldSpec, value: &Value, allow_http: bool) -> Result<Value, String> {
    match spec.kind {
        FieldKind::Text => match value {
            Value::String(text) => {
                let text = text.trim();
                if text.chars().count() > MAX_TEXT_CHARS {
                    Err(format!("must be at most {MAX_TEXT_CHARS} characters"))
                } else {
                    Ok(Value::String(text.to_owned()))
                }
            }
            _ => Err("must be a string".to_owned()),
        },
        FieldKind::Url => match value {
            Value::String(text) => match Url::parse(text.trim()) {
                Ok(url)
                    if url.host().is_some()
                        && (url.scheme() == "https" || (allow_http && url.scheme() == "http"))
                        && text.trim().chars().count() <= 2000 =>
                {
                    Ok(Value::String(text.trim().to_owned()))
                }
                Ok(url) if url.scheme() == "http" && url.host().is_some() && !allow_http => {
                    Err("must be an https URL".to_owned())
                }
                _ => Err("must be an http or https URL".to_owned()),
            },
            _ => Err("must be a string".to_owned()),
        },
        FieldKind::Select => match value {
            Value::String(text) if spec.options.contains(&text.as_str()) => {
                Ok(Value::String(text.clone()))
            }
            Value::String(_) => Err(format!("must be one of {}", spec.options.join(", "))),
            _ => Err("must be a string".to_owned()),
        },
        FieldKind::Number => match value {
            Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => {
                Ok(Value::Number(number.clone()))
            }
            _ => Err("must be a number".to_owned()),
        },
        FieldKind::Bool => match value {
            Value::Bool(flag) => Ok(Value::Bool(*flag)),
            _ => Err("must be a boolean".to_owned()),
        },
        FieldKind::Secret => Err("is a secret and cannot be set as a setting".to_owned()),
    }
}

pub fn validate_settings(
    descriptor: &SourceProviderDescriptor,
    input: &Map<String, Value>,
    allow_http: bool,
) -> Result<Map<String, Value>, Vec<FieldProblem>> {
    let mut problems = Vec::new();
    let mut settings = Map::new();
    for key in input.keys() {
        match descriptor.field(key) {
            None => problems.push(FieldProblem::new(key, "unknown field")),
            Some(spec) if spec.is_secret() => problems.push(FieldProblem::new(
                key,
                "is a secret and cannot be set as a setting",
            )),
            Some(_) => {}
        }
    }
    for spec in descriptor.setting_fields() {
        let provided = match input.get(spec.key) {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) if text.trim().is_empty() => None,
            Some(value) => Some(value),
        };
        match provided {
            Some(value) => match check(spec, value, allow_http) {
                Ok(normalised) => {
                    settings.insert(spec.key.to_owned(), normalised);
                }
                Err(message) => problems.push(FieldProblem::new(spec.key, message)),
            },
            None => match default_value(spec) {
                Some(value) => {
                    settings.insert(spec.key.to_owned(), value);
                }
                None if spec.required => problems.push(FieldProblem::new(spec.key, "is required")),
                None => {}
            },
        }
    }
    if problems.is_empty() {
        Ok(settings)
    } else {
        Err(problems)
    }
}

pub fn missing_secrets(
    descriptor: &SourceProviderDescriptor,
    present: &BTreeSet<&str>,
) -> Vec<FieldProblem> {
    descriptor
        .secret_fields()
        .filter(|spec| spec.required && !present.contains(spec.key))
        .map(|spec| FieldProblem::new(spec.key, "is required"))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::source_provider::catalog::descriptor;

    fn object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }

    fn france_travail() -> &'static SourceProviderDescriptor {
        descriptor("france_travail").expect("catalog entry")
    }

    #[test]
    fn defaults_are_applied_to_missing_optional_fields() {
        let settings =
            validate_settings(france_travail(), &object(json!({"client_id": "abc"})), true)
                .expect("valid");
        assert_eq!(settings["client_id"], "abc");
        assert!(settings["token_url"].as_str().is_some());
        assert!(settings["api_base_url"].as_str().is_some());
    }

    #[test]
    fn a_required_field_without_default_is_reported() {
        let problems = validate_settings(france_travail(), &Map::new(), true).unwrap_err();
        assert_eq!(
            problems,
            vec![FieldProblem::new("client_id", "is required")]
        );
    }

    #[test]
    fn blank_required_text_is_missing() {
        let problems =
            validate_settings(france_travail(), &object(json!({"client_id": "   "})), true)
                .unwrap_err();
        assert_eq!(problems[0].field, "client_id");
    }

    #[test]
    fn unknown_fields_and_secrets_in_settings_are_rejected() {
        let problems = validate_settings(
            france_travail(),
            &object(json!({"client_id": "a", "other": 1, "client_secret": "x"})),
            true,
        )
        .unwrap_err();
        let fields: Vec<&str> = problems.iter().map(|p| p.field.as_str()).collect();
        assert!(fields.contains(&"other"));
        assert!(fields.contains(&"client_secret"));
    }

    #[test]
    fn urls_must_be_http_with_a_host() {
        for bad in ["ftp://example.com", "not a url", "https://"] {
            let problems = validate_settings(
                france_travail(),
                &object(json!({"client_id": "a", "token_url": bad})),
                true,
            )
            .unwrap_err();
            assert_eq!(problems[0].field, "token_url", "{bad}");
        }
    }

    #[test]
    fn plain_http_is_refused_unless_allowed() {
        let input = object(json!({"client_id": "a", "token_url": "http://example.com/token"}));
        let problems = validate_settings(france_travail(), &input, false).unwrap_err();
        assert_eq!(problems[0].message, "must be an https URL");
        assert!(validate_settings(france_travail(), &input, true).is_ok());
    }

    #[test]
    fn wrong_types_are_reported() {
        let problems = validate_settings(france_travail(), &object(json!({"client_id": 12})), true)
            .unwrap_err();
        assert_eq!(problems[0].message, "must be a string");
    }

    #[test]
    fn missing_required_secrets_are_listed() {
        let none = BTreeSet::new();
        let missing = missing_secrets(france_travail(), &none);
        assert_eq!(
            missing,
            vec![FieldProblem::new("client_secret", "is required")]
        );
        let present = BTreeSet::from(["client_secret"]);
        assert!(missing_secrets(france_travail(), &present).is_empty());
    }

    fn spec(kind: FieldKind) -> FieldSpec {
        FieldSpec {
            kind,
            ..FieldSpec::text("field", "Champ", false).with_options(&["a", "b"])
        }
    }

    #[test]
    fn select_values_must_be_listed() {
        let select = spec(FieldKind::Select);
        assert!(check(&select, &json!("b"), true).is_ok());
        assert!(check(&select, &json!("c"), true).is_err());
    }

    #[test]
    fn numbers_must_be_json_numbers() {
        let number = spec(FieldKind::Number);
        assert!(check(&number, &json!(3), true).is_ok());
        assert!(check(&number, &json!("3"), true).is_err());
    }

    #[test]
    fn booleans_must_be_json_booleans() {
        let flag = spec(FieldKind::Bool);
        assert!(check(&flag, &json!(true), true).is_ok());
        assert!(check(&flag, &json!(1), true).is_err());
    }
}
