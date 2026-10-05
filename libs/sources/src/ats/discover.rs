use std::sync::LazyLock;

use lodestone_core::NormalizedDomain;
use regex::Regex;
use url::Url;

const SLUG: &str = r"([A-Za-z0-9][A-Za-z0-9._-]{0,99})";
const MAX_LINKED_PAGES: usize = 2;
const CAREER_WORDS: [&str; 8] = [
    "career", "carrier", "recrut", "emploi", "join-us", "joinus", "hiring", "jobs",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vendor {
    Greenhouse,
    Lever,
    LeverEu,
    Ashby,
}

impl Vendor {
    pub fn label(self) -> &'static str {
        match self {
            Self::Greenhouse => "greenhouse",
            Self::Lever | Self::LeverEu => "lever",
            Self::Ashby => "ashby",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Board {
    pub vendor: Vendor,
    pub slug: String,
}

impl Board {
    pub fn api_url(&self) -> Option<Url> {
        let slug = &self.slug;
        let raw = match self.vendor {
            Vendor::Greenhouse => {
                format!("https://boards-api.greenhouse.io/v1/boards/{slug}/jobs?content=true")
            }
            Vendor::Lever => format!("https://api.lever.co/v0/postings/{slug}?mode=json"),
            Vendor::LeverEu => format!("https://api.eu.lever.co/v0/postings/{slug}?mode=json"),
            Vendor::Ashby => format!(
                "https://api.ashbyhq.com/posting-api/job-board/{slug}?includeCompensation=false"
            ),
        };
        Url::parse(&raw).ok()
    }
}

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => unreachable!("static pattern {pattern} is invalid: {error}"),
    }
}

static GREENHOUSE_EMBED: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r#"(?i)//(?:boards|job-boards)(?:\.eu)?\.greenhouse\.io/embed/job_board(?:/js)?\?[^"'\s<>]*?\bfor={SLUG}"#
    ))
});
static GREENHOUSE_API: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r"(?i)//boards-api\.greenhouse\.io/v1/boards/{SLUG}"
    ))
});
static GREENHOUSE_BOARD: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r"(?i)//(?:boards|job-boards)(?:\.eu)?\.greenhouse\.io/{SLUG}"
    ))
});
static LEVER: LazyLock<Regex> =
    LazyLock::new(|| compile(&format!(r"(?i)//jobs(\.eu)?\.lever\.co/{SLUG}")));
static ASHBY: LazyLock<Regex> =
    LazyLock::new(|| compile(&format!(r"(?i)//jobs\.ashbyhq\.com/{SLUG}")));
static ASHBY_API: LazyLock<Regex> = LazyLock::new(|| {
    compile(&format!(
        r"(?i)//api\.ashbyhq\.com/posting-api/job-board/{SLUG}"
    ))
});
static HREF: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?i)\bhref\s*=\s*(?:"([^"]*)"|'([^']*)')"#));

fn normalize(text: &str) -> String {
    text.replace("\\u002F", "/")
        .replace("\\u002f", "/")
        .replace("\\/", "/")
        .replace("&#x2F;", "/")
        .replace("&#47;", "/")
        .replace("&amp;", "&")
}

fn clean_slug(raw: &str) -> Option<String> {
    let slug = raw.trim_end_matches('.');
    (!slug.is_empty()).then(|| slug.to_owned())
}

fn is_reserved_greenhouse(slug: &str) -> bool {
    matches!(slug.to_ascii_lowercase().as_str(), "embed" | "v1")
}

fn push(found: &mut Vec<(usize, Board)>, position: usize, vendor: Vendor, raw: &str) {
    let Some(slug) = clean_slug(raw) else {
        return;
    };
    let board = Board { vendor, slug };
    if !found.iter().any(|(_, known)| *known == board) {
        found.push((position, board));
    }
}

pub fn extract_boards(html: &str) -> Vec<Board> {
    let text = normalize(html);
    let mut found: Vec<(usize, Board)> = Vec::new();
    for caps in GREENHOUSE_EMBED.captures_iter(&text) {
        if let (Some(whole), Some(slug)) = (caps.get(0), caps.get(1))
            && !is_reserved_greenhouse(slug.as_str())
        {
            push(&mut found, whole.start(), Vendor::Greenhouse, slug.as_str());
        }
    }
    for caps in GREENHOUSE_API.captures_iter(&text) {
        if let (Some(whole), Some(slug)) = (caps.get(0), caps.get(1))
            && !is_reserved_greenhouse(slug.as_str())
        {
            push(&mut found, whole.start(), Vendor::Greenhouse, slug.as_str());
        }
    }
    for caps in GREENHOUSE_BOARD.captures_iter(&text) {
        if let (Some(whole), Some(slug)) = (caps.get(0), caps.get(1))
            && !is_reserved_greenhouse(slug.as_str())
        {
            push(&mut found, whole.start(), Vendor::Greenhouse, slug.as_str());
        }
    }
    for caps in LEVER.captures_iter(&text) {
        if let (Some(whole), Some(slug)) = (caps.get(0), caps.get(2)) {
            let vendor = if caps.get(1).is_some() {
                Vendor::LeverEu
            } else {
                Vendor::Lever
            };
            push(&mut found, whole.start(), vendor, slug.as_str());
        }
    }
    for pattern in [&*ASHBY, &*ASHBY_API] {
        for caps in pattern.captures_iter(&text) {
            if let (Some(whole), Some(slug)) = (caps.get(0), caps.get(1)) {
                push(&mut found, whole.start(), Vendor::Ashby, slug.as_str());
            }
        }
    }
    found.sort_by_key(|(position, _)| *position);
    found.into_iter().map(|(_, board)| board).collect()
}

fn looks_like_careers(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    CAREER_WORDS
        .iter()
        .any(|word| path.contains(word) || host.starts_with(word))
}

pub fn careers_links(html: &str, base: &Url, domain: &NormalizedDomain) -> Vec<Url> {
    let mut links: Vec<Url> = Vec::new();
    for caps in HREF.captures_iter(html) {
        let Some(raw) = caps.get(1).or_else(|| caps.get(2)) else {
            continue;
        };
        let Ok(mut url) = base.join(&normalize(raw.as_str())) else {
            continue;
        };
        url.set_fragment(None);
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }
        let same_site = url
            .host_str()
            .and_then(|host| NormalizedDomain::from_host(host).ok())
            .is_some_and(|host| host == *domain);
        if same_site && looks_like_careers(&url) && !links.contains(&url) {
            links.push(url);
            if links.len() == MAX_LINKED_PAGES {
                break;
            }
        }
    }
    links
}
