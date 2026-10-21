use std::num::NonZeroU32;

use common::OutboundConfig;
use fetch::FetchSettings;

pub fn ct_fetch_settings(
    outbound: &OutboundConfig,
    requests_per_host_per_minute: NonZeroU32,
) -> FetchSettings {
    let mut settings = FetchSettings::from_outbound(outbound);
    settings.requests_per_host_per_minute = requests_per_host_per_minute;
    settings
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use common::OutboundContact;
    use fetch::PoliteFetcher;
    use httpmock::{Method::GET, MockServer};
    use lodestone_core::{FetchError, FetchPurpose, FetchRequest, Fetcher, SourceName};
    use url::Url;

    use super::*;

    fn outbound() -> OutboundConfig {
        OutboundConfig {
            contact: OutboundContact::parse("bot@example.com").unwrap(),
            requests_per_host_per_minute: NonZeroU32::new(6).unwrap(),
            revisit_interval: Duration::from_secs(1),
            request_timeout: Duration::from_secs(5),
        }
    }

    fn local(mut settings: FetchSettings) -> PoliteFetcher {
        settings.allow_private_targets = true;
        PoliteFetcher::new(settings).unwrap()
    }

    #[test]
    fn ct_settings_override_only_the_rate() {
        let config = outbound();
        let shared = FetchSettings::from_outbound(&config);
        let ct = ct_fetch_settings(&config, NonZeroU32::new(120).unwrap());
        assert_eq!(ct.requests_per_host_per_minute.get(), 120);
        assert_eq!(shared.requests_per_host_per_minute.get(), 6);
        assert_eq!(ct.max_body_bytes, shared.max_body_bytes);
        assert_eq!(ct.contact, shared.contact);
    }

    #[tokio::test]
    async fn ct_host_is_not_throttled_by_the_shared_limit() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/tile");
                then.status(200);
            })
            .await;
        let config = outbound();
        let shared = local(FetchSettings::from_outbound(&config));
        let ct = local(ct_fetch_settings(&config, NonZeroU32::new(120).unwrap()));
        let request = FetchRequest {
            url: Url::parse(&server.url("/tile")).unwrap(),
            purpose: FetchPurpose::Api,
            source: SourceName::CertificateTransparency,
        };
        for _ in 0..30 {
            ct.fetch(&request).await.unwrap();
        }
        let mut limited = false;
        for _ in 0..10 {
            if matches!(
                shared.fetch(&request).await,
                Err(FetchError::RateLimited { .. })
            ) {
                limited = true;
            }
        }
        assert!(limited);
    }
}
