use lodestone_core::{NormalizedDomain, alpha3_of};
use url::Url;

use crate::buyer_domain::buyer_domain;

const COMPANY_MAX_CHARS: usize = 120;
const URL_TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'', '>'];
const URL_LEADING: &[char] = &['(', '[', '{', '"', '\'', '<'];

const EXCLUDED_HOSTS: [&str; 33] = [
    "ycombinator.com",
    "workatastartup.com",
    "hn.algolia.com",
    "lever.co",
    "greenhouse.io",
    "ashbyhq.com",
    "workable.com",
    "smartrecruiters.com",
    "recruitee.com",
    "breezy.hr",
    "bamboohr.com",
    "jobvite.com",
    "myworkdayjobs.com",
    "teamtailor.com",
    "personio.com",
    "personio.de",
    "wellfound.com",
    "angel.co",
    "remoteok.com",
    "remoteok.io",
    "weworkremotely.com",
    "stackoverflow.com",
    "levels.fyi",
    "builtin.com",
    "github.com",
    "gitlab.com",
    "twitter.com",
    "x.com",
    "medium.com",
    "youtube.com",
    "docs.google.com",
    "forms.gle",
    "bit.ly",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkMode {
    FullRemote,
    Hybrid,
    OnSite,
    Unknown,
}

impl WorkMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullRemote => "full_remote",
            Self::Hybrid => "hybrid",
            Self::OnSite => "onsite",
            Self::Unknown => "unknown",
        }
    }

    fn hint(self) -> Option<&'static str> {
        match self {
            Self::FullRemote => Some("Full remote."),
            Self::Hybrid => Some("Hybrid."),
            Self::OnSite => Some("On-site only, no remote."),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractHint {
    Freelance,
    FullTime,
    PartTime,
}

impl ContractHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Freelance => "freelance",
            Self::FullTime => "full_time",
            Self::PartTime => "part_time",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Freelance => "Freelance contract.",
            Self::FullTime => "Full-time employee.",
            Self::PartTime => "Part-time employee.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedComment {
    pub body: String,
    pub company: String,
    pub role: Option<String>,
    pub location: Vec<String>,
    pub work_mode: WorkMode,
    pub contracts: Vec<ContractHint>,
    pub country: Option<&'static str>,
    pub domain: Option<NormalizedDomain>,
}

impl ParsedComment {
    pub fn title(&self) -> String {
        match &self.role {
            Some(role) => format!("{role} - {}", self.company),
            None => self.company.clone(),
        }
    }

    pub fn text(&self, max_chars: usize) -> String {
        let mut text = String::new();
        if let Some(hint) = self.work_mode.hint() {
            text.push_str(hint);
            text.push('\n');
        }
        for contract in &self.contracts {
            text.push_str(contract.hint());
            text.push('\n');
        }
        text.push_str(&self.body);
        text.chars().take(max_chars).collect()
    }
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let decoded = after
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| decode_entity(&after[..end]).map(|letter| (letter, end)));
        match decoded {
            Some((letter, end)) => {
                out.push(letter);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn href_of(tag: &str) -> Option<String> {
    let start = tag.to_ascii_lowercase().find("href=")? + "href=".len();
    let value = &tag[start..];
    let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let inner = &value[1..];
    let end = inner.find(quote)?;
    Some(decode_entities(&inner[..end]))
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "br" | "li" | "pre" | "div" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    )
}

fn looks_like_url(text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || text.starts_with("http") || text.starts_with("www.")
}

pub fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut anchor: Option<(String, usize)> = None;
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(&decode_entities(&rest[..open]));
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            out.push('<');
            rest = after;
            continue;
        };
        let tag = &after[..close];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if name == "a" && !tag.starts_with('/') {
            anchor = href_of(tag).map(|href| (href, out.len()));
        } else if name == "a" {
            if let Some((href, start)) = anchor.take() {
                if looks_like_url(&out[start..]) {
                    out.truncate(start);
                    out.push(' ');
                    out.push_str(&href);
                    out.push(' ');
                } else {
                    out.push_str(" (");
                    out.push_str(&href);
                    out.push(')');
                }
            }
        } else if is_block(&name) {
            out.push('\n');
        }
        rest = &after[close + 1..];
    }
    out.push_str(&decode_entities(rest));
    out.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn url_of(word: &str) -> Option<Url> {
    let word = word
        .trim_start_matches(URL_LEADING)
        .trim_end_matches(URL_TRAILING);
    if word.starts_with("http://") || word.starts_with("https://") {
        Url::parse(word).ok()
    } else if word.starts_with("www.") {
        Url::parse(&format!("https://{word}")).ok()
    } else {
        None
    }
}

pub fn urls(text: &str) -> Vec<Url> {
    text.split(|c: char| c.is_whitespace() || c == '|')
        .filter_map(url_of)
        .collect()
}

fn matches_suffix(host: &str, suffix: &str) -> bool {
    host == suffix
        || host
            .strip_suffix(suffix)
            .is_some_and(|rest| rest.ends_with('.'))
}

pub fn is_excluded_host(host: &str) -> bool {
    host == "news.ycombinator.com"
        || EXCLUDED_HOSTS
            .iter()
            .any(|suffix| matches_suffix(host, suffix))
}

pub fn first_domain(text: &str) -> Option<NormalizedDomain> {
    urls(text).iter().find_map(|url| {
        let host = url.host_str()?.to_lowercase();
        if is_excluded_host(&host) {
            return None;
        }
        buyer_domain(&host)
    })
}

fn is_url_token(token: &str) -> bool {
    url_of(token).is_some()
}

fn mentions(token: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| token.contains(phrase))
}

const ONSITE: [&str; 3] = ["onsite", "on-site", "on site"];
const NEGATED_REMOTE: [&str; 3] = ["no remote", "not remote", "non-remote"];
const FULL_TIME: [&str; 3] = ["full-time", "full time", "fulltime"];
const PART_TIME: [&str; 3] = ["part-time", "part time", "parttime"];
const FREELANCE: [&str; 3] = ["contract", "freelance", "consultan"];

fn work_mode(tokens: &[String]) -> WorkMode {
    let (mut remote, mut onsite, mut hybrid, mut soft) = (false, false, false, false);
    for token in tokens {
        if mentions(token, &NEGATED_REMOTE) {
            onsite = true;
            continue;
        }
        hybrid |= token.contains("hybrid");
        onsite |= mentions(token, &ONSITE);
        if token.contains("remote") {
            remote = true;
            soft |= mentions(token, &["friendly", "optional", "possible"]);
        }
    }
    if hybrid || (remote && onsite) || (remote && soft) {
        WorkMode::Hybrid
    } else if remote {
        WorkMode::FullRemote
    } else if onsite {
        WorkMode::OnSite
    } else {
        WorkMode::Unknown
    }
}

fn contracts(tokens: &[String]) -> Vec<ContractHint> {
    let mut found = Vec::new();
    for (phrases, hint) in [
        (&FREELANCE, ContractHint::Freelance),
        (&FULL_TIME, ContractHint::FullTime),
        (&PART_TIME, ContractHint::PartTime),
    ] {
        if tokens.iter().any(|token| mentions(token, phrases)) {
            found.push(hint);
        }
    }
    found
}

const MODE_WORDS: [&str; 33] = [
    "remote",
    "onsite",
    "on-site",
    "on",
    "site",
    "hybrid",
    "full-time",
    "fulltime",
    "full",
    "part-time",
    "parttime",
    "part",
    "time",
    "contract",
    "contractor",
    "freelance",
    "only",
    "friendly",
    "optional",
    "possible",
    "ok",
    "or",
    "and",
    "no",
    "not",
    "us",
    "usa",
    "uk",
    "eu",
    "global",
    "worldwide",
    "anywhere",
    "office",
];

fn is_pure_token(lowered: &str) -> bool {
    let mut words = lowered
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|word| !word.is_empty())
        .peekable();
    words.peek().is_some() && words.all(|word| MODE_WORDS.contains(&word))
}

fn country_of(tokens: &[&str]) -> Option<&'static str> {
    tokens.iter().find_map(|token| {
        token
            .split(|c: char| !c.is_alphanumeric() && c != '.')
            .map(|word| word.trim_matches('.'))
            .find_map(|word| match word {
                "US" | "USA" | "U.S" | "U.S.A" => alpha3_of("US"),
                "UK" | "U.K" => alpha3_of("GB"),
                _ => None,
            })
    })
}

struct Header {
    company: String,
    role: Option<String>,
    location: Vec<String>,
    work_mode: WorkMode,
    contracts: Vec<ContractHint>,
    country: Option<&'static str>,
}

fn header_of(line: &str) -> Header {
    let tokens: Vec<&str> = line
        .split('|')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    let company_token = tokens.first().copied().unwrap_or_default();
    let company: String = company_token.chars().take(COMPANY_MAX_CHARS).collect();
    let details = tokens.get(1..).unwrap_or_default();
    let lowered: Vec<String> = details.iter().map(|token| token.to_lowercase()).collect();
    let mut role = None;
    let mut location = Vec::new();
    for (token, lower) in details.iter().zip(&lowered) {
        if is_url_token(token) || is_pure_token(lower) {
            continue;
        }
        if role.is_none() {
            role = Some((*token).to_owned());
        } else {
            location.push((*token).to_owned());
        }
    }
    Header {
        company,
        role,
        location,
        work_mode: work_mode(&lowered),
        contracts: contracts(&lowered),
        country: country_of(details),
    }
}

pub fn parse(html: &str) -> Option<ParsedComment> {
    let body = plain_text(html);
    let first = body.lines().next()?;
    let header = header_of(first);
    if header.company.is_empty() {
        return None;
    }
    let domain = first_domain(&body);
    Some(ParsedComment {
        body,
        company: header.company,
        role: header.role,
        location: header.location,
        work_mode: header.work_mode,
        contracts: header.contracts,
        country: header.country,
        domain,
    })
}

#[cfg(test)]
mod tests;
