use common::Secret;
use serde_json::{Map, Value, from_str};
use url::Url;

pub const CHUNK_PATH: &str = "/_next/static/chunks/";
const APPLICATION_ID: &str = "PUBLIC_ALGOLIA_APPLICATION_ID";
const SEARCH_KEY: &str = "PUBLIC_ALGOLIA_API_KEY_CLIENT";
const ORGANIZATIONS_INDEX: &str = "PUBLIC_ALGOLIA_ORGANIZATIONS_INDEX";
const MAX_APPLICATION_ID_CHARS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgoliaEnv {
    pub application_id: String,
    pub search_key: Secret,
    pub organizations_index: Option<String>,
    pub jobs_index: Option<String>,
}

fn env_object(body: &str) -> Option<Map<String, Value>> {
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    if end < start {
        return None;
    }
    match from_str::<Value>(&body[start..=end]).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn named<'a>(env: &'a Map<String, Value>, exact: &str, parts: &[&str]) -> Option<&'a str> {
    let text = |value: &'a Value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    env.get(exact).and_then(text).or_else(|| {
        env.iter()
            .filter(|(key, _)| {
                key.contains("ALGOLIA") && parts.iter().all(|part| key.contains(part))
            })
            .find_map(|(_, value)| text(value))
    })
}

pub fn valid_application_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_APPLICATION_ID_CHARS
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

pub fn parse_env(body: &[u8]) -> Option<AlgoliaEnv> {
    let env = env_object(&String::from_utf8_lossy(body))?;
    let application_id = named(&env, APPLICATION_ID, &["APPLICATION_ID"])
        .or_else(|| named(&env, APPLICATION_ID, &["APP_ID"]))
        .filter(|id| valid_application_id(id))?;
    let search_key = env
        .get(SEARCH_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .or_else(|| {
            env.iter()
                .filter(|(key, _)| {
                    key.contains("ALGOLIA") && key.contains("KEY") && !key.contains("ADMIN")
                })
                .find_map(|(_, value)| value.as_str().map(str::trim).filter(|key| !key.is_empty()))
        })?;
    Some(AlgoliaEnv {
        application_id: application_id.to_owned(),
        search_key: Secret::new(search_key),
        organizations_index: named(&env, ORGANIZATIONS_INDEX, &["ORGANIZATIONS", "INDEX"])
            .map(str::to_owned),
        jobs_index: named(&env, "PUBLIC_ALGOLIA_JOBS_INDEX", &["JOBS", "INDEX"]).map(str::to_owned),
    })
}

pub fn search_base(application_id: &str) -> Option<Url> {
    if !valid_application_id(application_id) {
        return None;
    }
    Url::parse(&format!(
        "https://{}-dsn.algolia.net/",
        application_id.to_ascii_lowercase()
    ))
    .ok()
}

fn is_chunk_char(letter: char) -> bool {
    letter.is_ascii_alphanumeric() || matches!(letter, '_' | '-' | '.' | '~')
}

pub fn script_urls(html: &str, site: &Url) -> Vec<Url> {
    let mut found: Vec<Url> = Vec::new();
    for (start, _) in html.match_indices(CHUNK_PATH) {
        let rest = &html[start + CHUNK_PATH.len()..];
        let length = rest
            .char_indices()
            .find(|(_, letter)| !is_chunk_char(*letter))
            .map_or(rest.len(), |(index, _)| index);
        let name = &rest[..length];
        if !name.ends_with(".js") {
            continue;
        }
        if let Ok(url) = site.join(&format!("{CHUNK_PATH}{name}"))
            && !found.contains(&url)
        {
            found.push(url);
        }
    }
    found.reverse();
    found
}

fn is_index_name(literal: &str) -> bool {
    !literal.is_empty()
        && literal
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && literal.split('_').any(|token| token == "jobs")
        && literal.split('_').any(|token| token == "production")
}

pub fn jobs_index_in(script: &str, language: &str) -> Option<String> {
    let mut names: Vec<String> = script
        .split(['"', '\'', '`'])
        .filter(|literal| is_index_name(literal))
        .map(|literal| {
            if literal.ends_with('_') {
                format!("{literal}{language}")
            } else {
                literal.to_owned()
            }
        })
        .collect();
    let suffix = format!("_{language}");
    names.sort_by_key(|name| (!name.ends_with(&suffix), name.len()));
    names.into_iter().next()
}
