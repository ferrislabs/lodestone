use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ContactFinder, ContactMethod, EmailAddress, FetchError, FetchPurpose, FetchRequest,
    FetchResponse, Fetcher, FoundAddress, NormalizedDomain, Provenance, SourceError, SourceName,
    UnitInterval,
};
use url::Url;

const LOCATIONS: [&str; 2] = ["/.well-known/security.txt", "/security.txt"];
const HINT_CONFIDENCE: f64 = 0.7;
const SIGNATURE_START: &str = "-----BEGIN PGP SIGNATURE-----";
const SIGNED_MESSAGE_START: &str = "-----BEGIN PGP SIGNED MESSAGE-----";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecurityTxt {
    pub contacts: Vec<String>,
    pub expires: Option<DateTime<Utc>>,
}

impl SecurityTxt {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires.is_some_and(|expires| expires <= now)
    }

    pub fn email_contacts(&self) -> Vec<EmailAddress> {
        self.contacts
            .iter()
            .filter_map(|contact| {
                let (scheme, rest) = contact.split_once(':')?;
                if !scheme.eq_ignore_ascii_case("mailto") {
                    return None;
                }
                let address = rest.split(['?', '#']).next()?;
                EmailAddress::parse(&percent_decode(address)).ok()
            })
            .collect()
    }
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(hex) = raw.get(index + 1..index + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            decoded.push(byte);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

pub fn parse_security_txt(text: &str) -> SecurityTxt {
    let mut parsed = SecurityTxt::default();
    let mut in_signed_header = false;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line == SIGNED_MESSAGE_START {
            in_signed_header = true;
            continue;
        }
        if in_signed_header {
            if line.is_empty() {
                in_signed_header = false;
            }
            continue;
        }
        if line == SIGNATURE_START {
            break;
        }
        let line = line.strip_prefix("- ").unwrap_or(line);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if field.trim().eq_ignore_ascii_case("contact") && !value.is_empty() {
            parsed.contacts.push(value.to_owned());
        } else if field.trim().eq_ignore_ascii_case("expires")
            && let Ok(expires) = DateTime::parse_from_rfc3339(value)
        {
            parsed.expires = Some(expires.with_timezone(&Utc));
        }
    }
    parsed
}

pub struct SecurityTxtFinder<F: Fetcher> {
    fetcher: Arc<F>,
    max_rate_limit_wait: Duration,
}

impl<F: Fetcher> SecurityTxtFinder<F> {
    pub fn new(fetcher: Arc<F>, max_rate_limit_wait: Duration) -> Self {
        Self {
            fetcher,
            max_rate_limit_wait,
        }
    }

    async fn fetch_waiting(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        match self.fetcher.fetch(request).await {
            Err(FetchError::RateLimited { retry_after })
                if retry_after <= self.max_rate_limit_wait =>
            {
                tokio::time::sleep(retry_after).await;
                self.fetcher.fetch(request).await
            }
            other => other,
        }
    }
}

fn is_plain_text(response: &FetchResponse) -> bool {
    response
        .header("content-type")
        .is_none_or(|value| value.to_ascii_lowercase().starts_with("text/plain"))
}

impl<F: Fetcher> ContactFinder for SecurityTxtFinder<F> {
    fn name(&self) -> SourceName {
        SourceName::SecurityTxt
    }

    async fn find(
        &self,
        domain: &NormalizedDomain,
        now: DateTime<Utc>,
    ) -> Result<Vec<FoundAddress>, SourceError> {
        let confidence = UnitInterval::new(HINT_CONFIDENCE)
            .map_err(|error| SourceError::Upstream(error.to_string()))?;
        for location in LOCATIONS {
            let Ok(url) = Url::parse(&format!("https://{}{location}", domain.as_str())) else {
                continue;
            };
            let request = FetchRequest {
                url,
                purpose: FetchPurpose::Probe,
                source: SourceName::SecurityTxt,
            };
            let response = match self.fetch_waiting(&request).await {
                Ok(response) => response,
                Err(FetchError::RateLimited { retry_after }) => {
                    return Err(SourceError::RateLimited { retry_after });
                }
                Err(error) => {
                    tracing::debug!(url = %request.url, %error, "security.txt location skipped");
                    continue;
                }
            };
            if response.status != 200 || !is_plain_text(&response) {
                continue;
            }
            let parsed = parse_security_txt(&response.text());
            if parsed.contacts.is_empty() || parsed.is_expired(now) {
                continue;
            }
            let Ok(provenance) = Provenance::parse(response.final_url.as_str()) else {
                continue;
            };
            return Ok(parsed
                .email_contacts()
                .into_iter()
                .map(|address| FoundAddress {
                    address,
                    provenance: provenance.clone(),
                    method: ContactMethod::SecurityTxt,
                    confidence,
                })
                .collect());
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests;
