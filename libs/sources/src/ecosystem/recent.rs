use std::{cmp::Reverse, sync::Arc, sync::LazyLock};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use lodestone_core::{
    EcosystemFinding, EcosystemModule, FetchError, FetchPurpose, FetchRequest, FetchResponse,
    Fetcher, InspectionTarget, NormalizedDomain, SourceError, SourceName,
};
use regex::Regex;
use serde_json::json;
use tracing::debug;
use url::Url;

use super::website::{WebsiteSettings, page_links, rate_limited, same_site};
use crate::{
    ats::jobs::html_to_text,
    contact_pages::extract::decode_entities,
    link_check::{LinkChecker, LinkOrigin, LinkVerdict, fetch_waiting},
};

pub const RECENT_DAYS: i64 = 90;
pub const MAX_RECENT_ITEMS: usize = 10;
pub const MAX_CHILD_SITEMAPS: usize = 3;
pub const MAX_ROBOTS_SITEMAPS: usize = 2;
pub const MAX_BLOG_ATTEMPTS: usize = 3;
pub const MAX_TITLE_CHARS: usize = 160;
pub const BLOG_WORDS: [&str; 13] = [
    "blog",
    "news",
    "actualites",
    "actualite",
    "actus",
    "actu",
    "presse",
    "press",
    "articles",
    "journal",
    "insights",
    "magazine",
    "newsroom",
];
pub const BLOG_PATHS: [&str; 3] = ["/blog", "/actualites", "/news"];
pub const SITEMAP_HINTS: [&str; 6] = ["post", "news", "blog", "actu", "article", "press"];
pub const MONTHS: [(&str, u32); 30] = [
    ("janvier", 1),
    ("fevrier", 2),
    ("février", 2),
    ("mars", 3),
    ("avril", 4),
    ("mai", 5),
    ("juin", 6),
    ("juillet", 7),
    ("aout", 8),
    ("août", 8),
    ("septembre", 9),
    ("octobre", 10),
    ("novembre", 11),
    ("decembre", 12),
    ("décembre", 12),
    ("january", 1),
    ("february", 2),
    ("march", 3),
    ("april", 4),
    ("may", 5),
    ("june", 6),
    ("july", 7),
    ("august", 8),
    ("september", 9),
    ("october", 10),
    ("november", 11),
    ("december", 12),
    ("sept", 9),
    ("oct", 10),
    ("nov", 11),
];

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => unreachable!("static pattern {pattern} is invalid: {error}"),
    }
}

static ENTRY: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"(?is)<(?:\w+:)?(?:url|sitemap)\b[^>]*>(.*?)</(?:\w+:)?(?:url|sitemap)\s*>")
});
static LOC: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?is)<(?:\w+:)?loc\s*>(.*?)</(?:\w+:)?loc\s*>"));
static LASTMOD: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?is)<(?:\w+:)?(?:lastmod|publication_date)\s*>(.*?)</(?:\w+:)?(?:lastmod|publication_date)\s*>",
    )
});
static NEWS_TITLE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?is)<(?:\w+:)?title\s*>(.*?)</(?:\w+:)?title\s*>"));
static INDEX: LazyLock<Regex> = LazyLock::new(|| compile(r"(?i)<(?:\w+:)?sitemapindex\b"));
static CDATA: LazyLock<Regex> = LazyLock::new(|| compile(r"(?s)<!\[CDATA\[(.*?)\]\]>"));
static ROBOTS_SITEMAP: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?im)^\s*sitemap\s*:\s*(\S+)\s*$"));
static ARTICLE: LazyLock<Regex> = LazyLock::new(|| compile(r"(?is)<article\b.*?</article\s*>"));
static HEADING: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?is)<h[1-4]\b[^>]*>(.*?)</h[1-4]\s*>"));
static ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
    compile(r#"(?is)<a\s[^>]*?href\s*=\s*(?:"([^"]*)"|'([^']*)')[^>]*>(.*?)</a\s*>"#)
});
static TIME: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?is)<time\b[^>]*?datetime\s*=\s*["']([^"']+)["']"#));
static ISO_DATE: LazyLock<Regex> = LazyLock::new(|| compile(r"\b(20\d{2})-(\d{2})-(\d{2})\b"));
static SLASH_DATE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"\b(\d{1,2})[/.](\d{1,2})[/.](20\d{2})\b"));
static WORD_DATE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?i)\b(\d{1,2})(?:er|st|nd|rd|th)?\s+([a-zéû]+)\.?\s+(20\d{2})\b"));
static ENGLISH_DATE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?i)\b([a-z]+)\.?\s+(\d{1,2})(?:st|nd|rd|th)?,\s*(20\d{2})\b"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapEntry {
    pub url: String,
    pub lastmod: Option<DateTime<Utc>>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sitemap {
    Index(Vec<SitemapEntry>),
    Urls(Vec<SitemapEntry>),
}

fn xml_text(raw: &str) -> String {
    let unwrapped = CDATA.replace_all(raw, "$1");
    decode_entities(unwrapped.trim()).trim().to_owned()
}

pub fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    if let Ok(time) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(time.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(trimmed.get(..10).unwrap_or(trimmed), "%Y-%m-%d")
        .ok()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc())
}

pub fn parse_sitemap(xml: &str) -> Sitemap {
    let entries: Vec<SitemapEntry> = ENTRY
        .captures_iter(xml)
        .filter_map(|captures| {
            let body = captures.get(1)?.as_str();
            let url = xml_text(LOC.captures(body)?.get(1)?.as_str());
            if url.is_empty() {
                return None;
            }
            Some(SitemapEntry {
                url,
                lastmod: LASTMOD
                    .captures(body)
                    .and_then(|found| found.get(1))
                    .and_then(|found| parse_time(&xml_text(found.as_str()))),
                title: NEWS_TITLE
                    .captures(body)
                    .and_then(|found| found.get(1))
                    .map(|found| xml_text(found.as_str()))
                    .filter(|title| !title.is_empty()),
            })
        })
        .collect();
    if INDEX.is_match(xml) {
        Sitemap::Index(entries)
    } else {
        Sitemap::Urls(entries)
    }
}

pub fn robots_sitemaps(body: &str) -> Vec<String> {
    ROBOTS_SITEMAP
        .captures_iter(body)
        .filter_map(|captures| captures.get(1).map(|found| found.as_str().to_owned()))
        .take(MAX_ROBOTS_SITEMAPS)
        .collect()
}

pub fn child_sitemaps(entries: &[SitemapEntry]) -> Vec<&SitemapEntry> {
    let mut ranked: Vec<&SitemapEntry> = entries.iter().collect();
    ranked.sort_by(|left, right| {
        let hinted = |entry: &SitemapEntry| {
            let lowered = entry.url.to_ascii_lowercase();
            SITEMAP_HINTS.iter().any(|hint| lowered.contains(hint))
        };
        hinted(right)
            .cmp(&hinted(left))
            .then(right.lastmod.cmp(&left.lastmod))
    });
    ranked.truncate(MAX_CHILD_SITEMAPS);
    ranked
}

pub fn recent_entries(entries: &[SitemapEntry], now: DateTime<Utc>) -> Vec<&SitemapEntry> {
    let since = now - Duration::days(RECENT_DAYS);
    let until = now + Duration::days(1);
    let mut recent: Vec<&SitemapEntry> = entries
        .iter()
        .filter(|entry| {
            entry
                .lastmod
                .is_some_and(|lastmod| lastmod >= since && lastmod <= until)
        })
        .collect();
    recent.sort_by_key(|entry| Reverse(entry.lastmod));
    recent
}

fn first_segment(url: &Url) -> String {
    url.path()
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_blog_word(segment: &str) -> bool {
    BLOG_WORDS.iter().any(|word| {
        segment == *word
            || segment
                .strip_prefix(word)
                .is_some_and(|rest| rest.starts_with('-') || rest.starts_with('_'))
    })
}

pub fn blog_links(html: &str, base: &Url, domain: &NormalizedDomain) -> Vec<Url> {
    let mut links: Vec<Url> = Vec::new();
    for mut url in page_links(html, base, domain) {
        url.set_query(None);
        let segments = url.path().trim_matches('/').split('/').count();
        if segments == 1 && is_blog_word(&first_segment(&url)) && !links.contains(&url) {
            links.push(url);
        }
    }
    links
}

fn month_number(word: &str) -> Option<u32> {
    let lowered = word.to_lowercase();
    MONTHS
        .iter()
        .find(|(name, _)| *name == lowered)
        .map(|(_, number)| *number)
}

pub fn date_in(text: &str) -> Option<NaiveDate> {
    if let Some(found) = ISO_DATE.captures(text) {
        let day = NaiveDate::from_ymd_opt(
            found[1].parse().ok()?,
            found[2].parse().ok()?,
            found[3].parse().ok()?,
        );
        if day.is_some() {
            return day;
        }
    }
    if let Some(found) = WORD_DATE
        .captures_iter(text)
        .find(|found| month_number(&found[2]).is_some())
    {
        return NaiveDate::from_ymd_opt(
            found[3].parse().ok()?,
            month_number(&found[2])?,
            found[1].parse().ok()?,
        );
    }
    if let Some(found) = ENGLISH_DATE
        .captures_iter(text)
        .find(|found| month_number(&found[1]).is_some())
    {
        return NaiveDate::from_ymd_opt(
            found[3].parse().ok()?,
            month_number(&found[1])?,
            found[2].parse().ok()?,
        );
    }
    let found = SLASH_DATE.captures(text)?;
    NaiveDate::from_ymd_opt(
        found[3].parse().ok()?,
        found[2].parse().ok()?,
        found[1].parse().ok()?,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlogItem {
    pub url: Url,
    pub title: Option<String>,
    pub date: Option<NaiveDate>,
}

fn clean_title(raw: &str) -> Option<String> {
    let text = html_to_text(raw);
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
        return None;
    }
    Some(compact.chars().take(MAX_TITLE_CHARS).collect())
}

fn is_item_of(url: &Url, page: &Url) -> bool {
    let base = page.path().trim_end_matches('/');
    let path = url.path().trim_end_matches('/');
    path != base && path.len() > 1 && (path.starts_with(&format!("{base}/")) || base.is_empty())
}

fn anchors<'a>(html: &'a str, page: &Url, domain: &NormalizedDomain) -> Vec<(Url, &'a str)> {
    ANCHOR
        .captures_iter(html)
        .filter_map(|captures| {
            let raw = captures.get(1).or_else(|| captures.get(2))?.as_str();
            let mut url = page.join(raw.trim()).ok()?;
            url.set_fragment(None);
            if !matches!(url.scheme(), "http" | "https") || !same_site(&url, page, domain) {
                return None;
            }
            Some((url, captures.get(3).map_or("", |found| found.as_str())))
        })
        .collect()
}

fn date_of_block(block: &str) -> Option<NaiveDate> {
    TIME.captures(block)
        .and_then(|found| found.get(1))
        .and_then(|found| parse_time(found.as_str()))
        .map(|time| time.date_naive())
        .or_else(|| date_in(&html_to_text(block)))
}

pub fn blog_items(html: &str, page: &Url, domain: &NormalizedDomain) -> Vec<BlogItem> {
    let mut items: Vec<BlogItem> = Vec::new();
    for block in ARTICLE.find_iter(html).map(|found| found.as_str()) {
        let mut links = anchors(block, page, domain);
        let Some(index) = links
            .iter()
            .position(|(url, _)| is_item_of(url, page))
            .or_else(|| {
                links
                    .iter()
                    .position(|(url, _)| url.path() != page.path() && url.path() != "/")
            })
        else {
            continue;
        };
        let (url, text) = links.swap_remove(index);
        if items.iter().any(|item| item.url == url) {
            continue;
        }
        let title = HEADING
            .captures(block)
            .and_then(|found| found.get(1))
            .and_then(|found| clean_title(found.as_str()))
            .or_else(|| clean_title(text));
        items.push(BlogItem {
            url,
            title,
            date: date_of_block(block),
        });
    }
    if !items.is_empty() {
        return items;
    }
    for (url, text) in anchors(html, page, domain) {
        if !is_item_of(&url, page) || items.iter().any(|item| item.url == url) {
            continue;
        }
        let Some(title) = clean_title(text) else {
            continue;
        };
        items.push(BlogItem {
            url,
            title: Some(title),
            date: None,
        });
    }
    items
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentItem {
    pub url: Url,
    pub title: Option<String>,
    pub date: Option<NaiveDate>,
}

pub fn is_publication(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    let segments: Vec<String> = url
        .path()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    segments
        .split_last()
        .is_some_and(|(_, parents)| parents.iter().any(|segment| is_blog_word(segment)))
}

pub fn publications<'a>(recent: &[&'a SitemapEntry]) -> Vec<&'a SitemapEntry> {
    let posts: Vec<&SitemapEntry> = recent
        .iter()
        .copied()
        .filter(|entry| is_publication(&entry.url))
        .collect();
    if posts.is_empty() {
        recent.to_vec()
    } else {
        posts
    }
}

pub const TITLE_SUFFIX_SEPARATORS: [&str; 4] = [" | ", " – ", " — ", " - "];
const MAX_SUFFIX_CHARS: usize = 30;

pub fn without_site_suffix(title: &str) -> &str {
    TITLE_SUFFIX_SEPARATORS
        .iter()
        .filter_map(|separator| {
            let (head, tail) = title.rsplit_once(separator)?;
            (tail.chars().count() <= MAX_SUFFIX_CHARS
                && head.chars().count() >= MAX_SUFFIX_CHARS / 3)
                .then_some(head)
        })
        .min_by_key(|head| head.len())
        .unwrap_or(title)
}

pub fn merge_items(
    blog: &[BlogItem],
    sitemap: &[&SitemapEntry],
    resolve: impl Fn(&str) -> Option<Url>,
) -> Vec<RecentItem> {
    let mut items: Vec<RecentItem> = blog
        .iter()
        .map(|item| RecentItem {
            url: item.url.clone(),
            title: item.title.clone(),
            date: item.date,
        })
        .collect();
    for entry in sitemap {
        let Some(url) = resolve(&entry.url) else {
            continue;
        };
        if url.path() == "/" {
            continue;
        }
        let date = entry.lastmod.map(|time| time.date_naive());
        match items.iter_mut().find(|item| item.url == url) {
            Some(known) => {
                known.date = known.date.or(date);
                if known.title.is_none() {
                    known.title.clone_from(&entry.title);
                }
            }
            None => items.push(RecentItem {
                url,
                title: entry.title.clone(),
                date,
            }),
        }
    }
    items.sort_by_key(|item| Reverse(item.date));
    items
}

pub fn days_since(latest: NaiveDate, now: DateTime<Utc>) -> i64 {
    (now.date_naive() - latest).num_days().max(0)
}

pub struct RecentModule<F: Fetcher> {
    fetcher: Arc<F>,
    settings: WebsiteSettings,
}

struct SitemapRead {
    source: Option<String>,
    entries: Vec<SitemapEntry>,
}

impl<F: Fetcher> RecentModule<F> {
    pub fn new(fetcher: Arc<F>, settings: WebsiteSettings) -> Self {
        Self { fetcher, settings }
    }

    fn localized(&self, raw: &str) -> Option<Url> {
        let url = Url::parse(raw).ok()?;
        match &self.settings.origin {
            Some(origin) => {
                let mut local = origin.join(url.path()).ok()?;
                local.set_query(url.query());
                Some(local)
            }
            None => Some(url),
        }
    }

    async fn get(
        &self,
        url: &Url,
        purpose: FetchPurpose,
    ) -> Result<Option<FetchResponse>, SourceError> {
        let request = FetchRequest {
            url: url.clone(),
            purpose,
            source: SourceName::PublicPage,
        };
        match fetch_waiting(
            self.fetcher.as_ref(),
            &request,
            self.settings.max_rate_limit_wait,
        )
        .await
        {
            Ok(response) => Ok(Some(response)),
            Err(FetchError::RateLimited { retry_after }) => {
                Err(SourceError::RateLimited { retry_after })
            }
            Err(error) => {
                debug!(%url, %error, "recent activity page not read");
                Ok(None)
            }
        }
    }

    async fn sitemap(&self, url: &Url) -> Result<Option<Sitemap>, SourceError> {
        Ok(self
            .get(url, FetchPurpose::Crawl)
            .await?
            .filter(|response| response.status == 200)
            .map(|response| parse_sitemap(&response.text())))
    }

    async fn read_sitemaps(&self, domain: &NormalizedDomain) -> Result<SitemapRead, SourceError> {
        let mut roots: Vec<Url> = Vec::new();
        if let Some(robots) = self.settings.url(domain.as_str(), "/robots.txt")
            && let Some(response) = self.get(&robots, FetchPurpose::Api).await?
            && response.status == 200
        {
            for raw in robots_sitemaps(&response.text()) {
                let site = Url::parse(&raw)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
                    .and_then(|host| NormalizedDomain::from_host(&host).ok());
                if site.as_ref() == Some(domain)
                    && let Some(url) = self.localized(&raw)
                    && !roots.contains(&url)
                {
                    roots.push(url);
                }
            }
        }
        if roots.is_empty()
            && let Some(url) = self.settings.url(domain.as_str(), "/sitemap.xml")
        {
            roots.push(url);
        }
        for root in roots {
            let Some(sitemap) = self.sitemap(&root).await? else {
                continue;
            };
            let entries = match sitemap {
                Sitemap::Urls(entries) => entries,
                Sitemap::Index(children) => {
                    let mut entries = Vec::new();
                    for child in child_sitemaps(&children) {
                        let Some(url) = self.localized(&child.url) else {
                            continue;
                        };
                        if let Some(Sitemap::Urls(found)) = self.sitemap(&url).await? {
                            entries.extend(found);
                        }
                    }
                    entries
                }
            };
            if !entries.is_empty() {
                return Ok(SitemapRead {
                    source: Some(root.to_string()),
                    entries,
                });
            }
        }
        Ok(SitemapRead {
            source: None,
            entries: Vec::new(),
        })
    }

    pub async fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        let module = EcosystemModule::RecentActivity;
        let domain = target.domain;
        let now = target.now;
        let sitemap = self.read_sitemaps(domain).await?;
        let Some(home) = self.settings.url(domain.as_str(), "/") else {
            return Ok(Vec::new());
        };
        let mut checker = LinkChecker::new(
            self.fetcher.as_ref(),
            SourceName::PublicPage,
            self.settings.max_rate_limit_wait,
        );
        let mut blog: Option<(Url, Vec<BlogItem>)> = None;
        let mut base = home.clone();
        if let Some(response) = self.get(&home, FetchPurpose::Crawl).await?
            && matches!(
                checker
                    .confirm(&home, LinkOrigin::Home, &response)
                    .await
                    .map_err(rate_limited)?,
                LinkVerdict::Confirmed(_)
            )
        {
            base = response.final_url.clone();
            let mut candidates: Vec<(Url, LinkOrigin)> =
                blog_links(&response.text(), &base, domain)
                    .into_iter()
                    .map(|url| (url, LinkOrigin::Linked))
                    .collect();
            for path in BLOG_PATHS {
                if let Some(url) = self.settings.url(domain.as_str(), path)
                    && !candidates.iter().any(|(known, _)| *known == url)
                {
                    candidates.push((url, LinkOrigin::Guessed));
                }
            }
            for (url, origin) in candidates.into_iter().take(MAX_BLOG_ATTEMPTS) {
                let Some(page) = self.get(&url, FetchPurpose::Crawl).await? else {
                    continue;
                };
                let verdict = checker
                    .confirm(&url, origin, &page)
                    .await
                    .map_err(rate_limited)?;
                if matches!(verdict, LinkVerdict::Confirmed(_)) {
                    let items = blog_items(&page.text(), &page.final_url, domain);
                    blog = Some((page.final_url.clone(), items));
                    break;
                }
            }
        }
        let recent = recent_entries(&sitemap.entries, now);
        let blog_items_found = blog.as_ref().map_or(&[][..], |(_, items)| items.as_slice());
        let blog_page = blog.as_ref().map(|(url, _)| url.as_str());
        let resolve = |raw: &str| match &self.settings.origin {
            Some(_) => self.localized(raw),
            None => base.join(raw).ok(),
        };
        let published = publications(&recent);
        let candidates: Vec<RecentItem> = merge_items(blog_items_found, &published, resolve)
            .into_iter()
            .filter(|item| {
                Some(item.url.as_str()) != blog_page
                    && item.url != base
                    && item.url.path() != "/"
                    && same_site(&item.url, &base, domain)
            })
            .collect();
        let mut confirmed: Vec<RecentItem> = Vec::new();
        for mut item in candidates.into_iter().take(MAX_RECENT_ITEMS) {
            let page = match self.get(&item.url, FetchPurpose::Crawl).await {
                Ok(Some(page)) => page,
                Ok(None) => continue,
                Err(SourceError::RateLimited { retry_after }) => {
                    debug!(%domain, ?retry_after, "recent items cut short, host rate limited");
                    break;
                }
                Err(error) => return Err(error),
            };
            let verdict = match checker.confirm(&item.url, LinkOrigin::Linked, &page).await {
                Ok(verdict) => verdict,
                Err(FetchError::RateLimited { retry_after }) => {
                    debug!(%domain, ?retry_after, "recent items cut short, host rate limited");
                    break;
                }
                Err(error) => return Err(SourceError::Fetch(error)),
            };
            if let LinkVerdict::Confirmed(snapshot) = verdict {
                if item.title.is_none() {
                    item.title = clean_title(without_site_suffix(&snapshot.title));
                }
                if item.title.is_some() {
                    confirmed.push(item);
                }
            }
        }
        let mut findings: Vec<EcosystemFinding> = Vec::new();
        if let Some(latest) = confirmed.first()
            && let (Some(title), Some(date)) = (&latest.title, latest.date)
        {
            findings.push(
                EcosystemFinding::new(
                    module,
                    "latest_news",
                    "Dernière actualité",
                    format!("{title} ({date})"),
                    latest.url.as_str(),
                    now,
                )
                .keyed("latest_news")
                .with_details(json!({"title": title, "published_on": date.to_string()})),
            );
        }
        let newest_sitemap = sitemap
            .entries
            .iter()
            .filter_map(|entry| entry.lastmod)
            .filter(|lastmod| *lastmod <= now + Duration::days(1))
            .max()
            .map(|time| time.date_naive());
        let newest_item = confirmed.iter().filter_map(|item| item.date).max();
        let freshness = match (newest_sitemap, newest_item) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (left, right) => left.or(right),
        };
        if let Some(latest) = freshness {
            let days = days_since(latest, now);
            let source = sitemap
                .source
                .clone()
                .or_else(|| confirmed.first().map(|item| item.url.to_string()))
                .unwrap_or_default();
            let value = match days {
                0 => format!("mis à jour aujourd'hui ({latest})"),
                1 => format!("mis à jour il y a 1 jour ({latest})"),
                days => format!("mis à jour il y a {days} jours ({latest})"),
            };
            findings.push(
                EcosystemFinding::new(module, "freshness", "Fraîcheur du site", value, source, now)
                    .keyed("freshness")
                    .with_details(json!({"days": days, "latest": latest.to_string()})),
            );
        }
        if let Some(source) = &sitemap.source {
            findings.push(
                EcosystemFinding::new(
                    module,
                    "recent_pages",
                    "Pages mises à jour (90 jours)",
                    recent.len().to_string(),
                    source.as_str(),
                    now,
                )
                .keyed("recent_pages")
                .with_details(json!({
                    "count": recent.len(),
                    "listed": sitemap.entries.len(),
                    "days": RECENT_DAYS,
                })),
            );
        }
        if let Some(page) = blog_page {
            findings.push(
                EcosystemFinding::new(module, "blog_page", "Page actualités", page, page, now)
                    .keyed("blog_page"),
            );
        }
        for item in &confirmed {
            let Some(title) = &item.title else {
                continue;
            };
            findings.push(
                EcosystemFinding::new(
                    module,
                    "recent_item",
                    "Publication",
                    title.as_str(),
                    item.url.as_str(),
                    now,
                )
                .keyed(format!("recent_item:{}", item.url))
                .with_details(json!({"date": item.date.map(|date| date.to_string())})),
            );
        }
        Ok(findings)
    }
}

#[cfg(test)]
mod tests;
