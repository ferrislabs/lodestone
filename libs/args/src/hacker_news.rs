use std::{
    num::{NonZeroU32, NonZeroU64},
    time::Duration,
};

use url::Url;

#[derive(clap::Args, Debug, Clone)]
pub struct HackerNewsArgs {
    #[arg(
        long = "hacker-news-base-url",
        env = "HACKER_NEWS_BASE_URL",
        name = "HACKER_NEWS_BASE_URL",
        default_value = "https://hn.algolia.com/api/v1/",
        long_help = "Root of the Hacker News Algolia search API. Seeds the catalog configuration"
    )]
    pub base_url: Url,

    #[arg(
        long = "hacker-news-interval-hours",
        env = "HACKER_NEWS_INTERVAL_HOURS",
        name = "HACKER_NEWS_INTERVAL_HOURS",
        default_value = "24",
        long_help = "Hours between two Hacker News \"Who is hiring?\" runs"
    )]
    pub interval_hours: NonZeroU64,

    #[arg(
        long = "hacker-news-max-pages",
        env = "HACKER_NEWS_MAX_PAGES",
        name = "HACKER_NEWS_MAX_PAGES",
        default_value = "5",
        long_help = "Maximum pages of 100 comments fetched per keyword and thread"
    )]
    pub max_pages: NonZeroU32,
}

impl HackerNewsArgs {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_hours.get().saturating_mul(3600))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        hacker_news: HackerNewsArgs,
    }

    #[test]
    fn defaults() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(cmd.hacker_news.interval(), Duration::from_secs(24 * 3600));
        assert_eq!(cmd.hacker_news.max_pages.get(), 5);
        assert_eq!(cmd.hacker_news.base_url.host_str(), Some("hn.algolia.com"));
    }

    #[test]
    fn overrides_are_read() {
        let cmd = Cmd::try_parse_from(["cmd", "--hacker-news-interval-hours", "6"]).unwrap();
        assert_eq!(cmd.hacker_news.interval(), Duration::from_secs(6 * 3600));
    }

    #[test]
    fn zero_values_are_rejected() {
        assert!(Cmd::try_parse_from(["cmd", "--hacker-news-interval-hours", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--hacker-news-max-pages", "0"]).is_err());
    }
}
