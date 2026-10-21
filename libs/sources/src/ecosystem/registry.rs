use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use lodestone_core::{
    CompanyBuilder, FetchPurpose, FetchRequest, Fetcher, InspectionTarget, Siren, SirenLookup,
    SourceError, SourceName,
};
use url::Url;

use crate::{company::CompanyResearch, link_check::fetch_waiting};

pub const VALUE_CHARS: usize = 240;

pub async fn resolve_siren<F: Fetcher>(
    research: &CompanyResearch<F>,
    target: &InspectionTarget<'_>,
) -> Result<Option<Siren>, SourceError> {
    match target.siren {
        SirenLookup::Known(siren) => Ok(Some(siren.clone())),
        SirenLookup::Unknown => Ok(None),
        SirenLookup::Unresolved => Ok(research.build(target.domain).await?.siren),
    }
}

pub async fn read_api<F: Fetcher>(
    fetcher: &F,
    url: Url,
    max_wait: Duration,
    service: &str,
) -> Result<Vec<u8>, SourceError> {
    let request = FetchRequest {
        url,
        purpose: FetchPurpose::Api,
        source: SourceName::PublicPage,
    };
    let response = fetch_waiting(fetcher, &request, max_wait).await?;
    if response.status != 200 {
        return Err(SourceError::Upstream(format!(
            "{service} answered {}",
            response.status
        )));
    }
    if response.truncated {
        return Err(SourceError::Upstream(format!(
            "{service} response exceeded the body cap"
        )));
    }
    Ok(response.body)
}

pub fn shortened(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= VALUE_CHARS {
        return compact;
    }
    let mut cut: String = compact.chars().take(VALUE_CHARS - 1).collect();
    cut.push('…');
    cut
}

pub fn parse_day(raw: &str) -> Option<NaiveDate> {
    let trimmed = raw.trim();
    NaiveDate::parse_from_str(trimmed.get(..10).unwrap_or(trimmed), "%Y-%m-%d").ok()
}

pub fn day_start(date: NaiveDate) -> Option<DateTime<Utc>> {
    date.and_hms_opt(0, 0, 0).map(|naive| naive.and_utc())
}
