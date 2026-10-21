use super::extract;

const KEY: &str = "mistral-key-0123456789abcdef";

fn message(body: &str) -> Option<String> {
    extract(body.as_bytes(), Some(KEY))
}

#[test]
fn openai_error_object() {
    assert_eq!(
        message(r#"{"error":{"message":"Unsupported parameter: 'reasoning_effort'","type":"invalid_request_error"}}"#)
            .as_deref(),
        Some("Unsupported parameter: 'reasoning_effort'")
    );
}

#[test]
fn error_string() {
    assert_eq!(
        message(r#"{"error":"model 'x' not found"}"#).as_deref(),
        Some("model 'x' not found")
    );
}

#[test]
fn top_level_message() {
    assert_eq!(
        message(r#"{"message":"Invalid model"}"#).as_deref(),
        Some("Invalid model")
    );
}

#[test]
fn mistral_object_error() {
    assert_eq!(
        message(r#"{"object":"error","message":"Invalid schema","type":"invalid_request_error","code":3051}"#)
            .as_deref(),
        Some("Invalid schema")
    );
}

#[test]
fn detail_string() {
    assert_eq!(
        message(r#"{"detail":"Not authenticated"}"#).as_deref(),
        Some("Not authenticated")
    );
}

#[test]
fn detail_array_with_locations() {
    assert_eq!(
        message(r#"{"detail":[{"type":"extra_forbidden","loc":["body","reasoning_effort"],"msg":"Extra inputs are not permitted"},{"loc":["body","messages",0],"msg":"Field required"}]}"#)
            .as_deref(),
        Some("reasoning_effort: Extra inputs are not permitted; messages.0: Field required")
    );
}

#[test]
fn mistral_message_wrapping_detail() {
    assert_eq!(
        message(r#"{"object":"error","message":{"detail":[{"loc":["body","response_format"],"msg":"Input should be a valid dictionary"}]},"type":"invalid_request_error"}"#)
            .as_deref(),
        Some("response_format: Input should be a valid dictionary")
    );
}

#[test]
fn anthropic_error_envelope() {
    assert_eq!(
        message(r#"{"type":"error","error":{"type":"invalid_request_error","message":"output_config.format: schema is invalid"}}"#)
            .as_deref(),
        Some("output_config.format: schema is invalid")
    );
}

#[test]
fn plain_text_is_kept() {
    assert_eq!(
        message("upstream  connect\n error").as_deref(),
        Some("upstream connect error")
    );
}

#[test]
fn json_without_a_known_shape_falls_back_to_text() {
    assert_eq!(
        message(r#"{"code": 12}"#).as_deref(),
        Some(r#"{"code": 12}"#)
    );
}

#[test]
fn empty_and_blank_bodies_have_no_message() {
    assert_eq!(message(""), None);
    assert_eq!(message("  \n\t "), None);
}

#[test]
fn long_text_is_capped() {
    let long = "a".repeat(5_000);
    assert_eq!(message(&long).unwrap().chars().count(), 300);
}

#[test]
fn long_json_message_is_capped() {
    let body = format!(r#"{{"error":{{"message":"{}"}}}}"#, "b".repeat(2_000));
    assert_eq!(message(&body).unwrap().chars().count(), 300);
}

#[test]
fn cap_counts_characters() {
    let long = "é".repeat(400);
    assert_eq!(message(&long).unwrap().chars().count(), 300);
}

#[test]
fn configured_key_is_redacted() {
    let body = format!(r#"{{"error":{{"message":"Invalid key {KEY} provided"}}}}"#);
    assert_eq!(
        message(&body).as_deref(),
        Some("Invalid key [redacted] provided")
    );
}

#[test]
fn key_straddling_the_cap_is_not_leaked() {
    let padding = "x ".repeat(148);
    let body = format!("{padding}{KEY}");
    let found = message(&body).unwrap();
    assert!(!found.contains(&KEY[..8]));
}

#[test]
fn key_shaped_tokens_are_redacted() {
    assert_eq!(
        message("Incorrect API key provided: sk-proj-AbCdEf123456. See docs").as_deref(),
        Some("Incorrect API key provided: [redacted]. See docs")
    );
    assert_eq!(
        message("(sk-ant-api03-xxxxxxxxxxxx) rejected").as_deref(),
        Some("([redacted]) rejected")
    );
}

#[test]
fn masked_and_embedded_sk_prefixes_are_left_alone() {
    assert_eq!(
        message("task-based sk-abc***xyz").as_deref(),
        Some("task-based sk-abc***xyz")
    );
}

#[test]
fn bearer_tokens_are_redacted() {
    assert_eq!(
        message("Header Authorization: Bearer abc.def-123 is invalid").as_deref(),
        Some("Header Authorization: Bearer [redacted] is invalid")
    );
    assert_eq!(
        message("bearer ABCDEF").as_deref(),
        Some("bearer [redacted]")
    );
}

#[test]
fn non_utf8_bytes_do_not_panic() {
    let found = extract(&[0xff, 0xfe, b'o', b'k'], None).unwrap();
    assert!(found.ends_with("ok"));
}
