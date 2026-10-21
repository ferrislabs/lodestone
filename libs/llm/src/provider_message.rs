use reqwest::Response;
use serde_json::Value;

const BODY_LIMIT: usize = 16 * 1024;
const MESSAGE_LIMIT: usize = 300;
const DEPTH_LIMIT: usize = 4;
const REDACTED: &str = "[redacted]";
const SECRET_PREFIX: &str = "sk-";
const SECRET_MIN_TAIL: usize = 6;

pub(crate) async fn read(response: Response, key: Option<&str>) -> Option<String> {
    let mut response = response;
    let mut body: Vec<u8> = Vec::new();
    while body.len() < BODY_LIMIT {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let room = BODY_LIMIT - body.len();
                body.extend_from_slice(&chunk[..chunk.len().min(room)]);
            }
            Ok(None) | Err(_) => break,
        }
    }
    extract(&body, key)
}

pub(crate) fn extract(body: &[u8], key: Option<&str>) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    let raw = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| from_value(&value, 0))
        .unwrap_or_else(|| text.into_owned());
    let cleaned = clean(&raw, key);
    (!cleaned.is_empty()).then_some(cleaned)
}

fn from_value(value: &Value, depth: usize) -> Option<String> {
    if depth > DEPTH_LIMIT {
        return None;
    }
    match value {
        Value::String(text) => Some(text.clone()).filter(|text| !text.trim().is_empty()),
        Value::Object(map) => {
            if let Some(found) = ["error", "message", "detail"]
                .iter()
                .filter_map(|name| map.get(*name))
                .find_map(|inner| from_value(inner, depth + 1))
            {
                return Some(found);
            }
            map.get("msg")
                .and_then(|inner| from_value(inner, depth + 1))
        }
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(|item| entry(item, depth)).collect();
            (!parts.is_empty()).then(|| parts.join("; "))
        }
        _ => None,
    }
}

fn entry(item: &Value, depth: usize) -> Option<String> {
    let message = from_value(item, depth + 1)?;
    let location = item
        .get("loc")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| match part {
                    Value::String(text) => Some(text.clone()),
                    Value::Number(number) => Some(number.to_string()),
                    _ => None,
                })
                .filter(|part| part != "body")
                .collect::<Vec<_>>()
                .join(".")
        })
        .filter(|location| !location.is_empty());
    Some(match location {
        Some(location) => format!("{location}: {message}"),
        None => message,
    })
}

fn clean(raw: &str, key: Option<&str>) -> String {
    let keyed = match key {
        Some(secret) if !secret.is_empty() => raw.replace(secret, REDACTED),
        _ => raw.to_owned(),
    };
    let words: Vec<String> = keyed.split_whitespace().map(redact_secret_tokens).collect();
    let mut sanitized: Vec<String> = Vec::with_capacity(words.len());
    let mut after_bearer = false;
    for word in words {
        if after_bearer {
            after_bearer = false;
            if !word.contains(REDACTED) {
                sanitized.push(REDACTED.to_owned());
                continue;
            }
        }
        after_bearer = word.eq_ignore_ascii_case("bearer");
        sanitized.push(word);
    }
    sanitized
        .join(" ")
        .chars()
        .take(MESSAGE_LIMIT)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn is_token_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

fn redact_secret_tokens(word: &str) -> String {
    let mut output = String::with_capacity(word.len());
    let mut rest = word;
    let mut previous: Option<char> = None;
    while let Some(position) = rest.find(SECRET_PREFIX) {
        let before = rest[..position].chars().next_back().or(previous);
        let tail_start = position + SECRET_PREFIX.len();
        let tail = rest[tail_start..]
            .chars()
            .take_while(|character| is_token_char(*character))
            .count();
        output.push_str(&rest[..position]);
        if before.is_none_or(|character| !character.is_ascii_alphanumeric())
            && tail >= SECRET_MIN_TAIL
        {
            output.push_str(REDACTED);
            let consumed: usize = rest[tail_start..]
                .chars()
                .take(tail)
                .map(char::len_utf8)
                .sum();
            rest = &rest[tail_start + consumed..];
        } else {
            output.push_str(SECRET_PREFIX);
            rest = &rest[tail_start..];
        }
        previous = output.chars().next_back();
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests;
