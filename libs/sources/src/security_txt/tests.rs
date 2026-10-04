use std::{collections::HashMap, sync::Mutex};

use chrono::TimeZone;

use super::*;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap()
}

const SIGNED: &str = "-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA256

# Acme security contacts
Contact: mailto:security@acme.com
Contact: https://acme.com/security/report
Contact: mailto:psirt%40acme.com?subject=Vulnerability
Contact: tel:+33-1-23-45-67-89
Expires: 2027-01-01T00:00:00.000Z
Preferred-Languages: en, fr
-----BEGIN PGP SIGNATURE-----
Contact: mailto:forged@evil.example.org
-----END PGP SIGNATURE-----
";

#[test]
fn signed_file_yields_contacts_before_the_signature_only() {
    let parsed = parse_security_txt(SIGNED);
    assert_eq!(parsed.contacts.len(), 4);
    let emails: Vec<String> = parsed
        .email_contacts()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(emails, ["security@acme.com", "psirt@acme.com"]);
    assert_eq!(
        parsed.expires,
        Some(Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap())
    );
    assert!(!parsed.is_expired(now()));
}

#[test]
fn field_names_are_case_insensitive_and_comments_ignored() {
    let parsed =
        parse_security_txt("# contact: mailto:no@acme.com\nCONTACT:mailto:Sec@Acme.com\r\n");
    let emails: Vec<String> = parsed
        .email_contacts()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(emails, ["sec@acme.com"]);
}

#[test]
fn expired_file_is_detected() {
    let parsed = parse_security_txt("Contact: mailto:a@acme.com\nExpires: 2020-01-01T00:00:00Z\n");
    assert!(parsed.is_expired(now()));
}

#[test]
fn garbage_yields_nothing() {
    let parsed = parse_security_txt("<html><body>Not found</body></html>");
    assert!(parsed.contacts.is_empty());
}

type Reply = Result<(u16, &'static str, &'static str), FetchError>;

struct FakeFetcher {
    script: HashMap<&'static str, Reply>,
    requests: Mutex<Vec<(String, FetchPurpose)>>,
}

impl Fetcher for FakeFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let key = request.url.to_string();
        self.requests
            .lock()
            .unwrap()
            .push((key.clone(), request.purpose));
        match self.script.get(key.as_str()) {
            Some(Ok((status, content_type, body))) => Ok(FetchResponse {
                status: *status,
                headers: vec![("content-type".to_owned(), (*content_type).to_owned())],
                body: body.as_bytes().to_vec(),
                final_url: request.url.clone(),
                truncated: false,
            }),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(FetchResponse {
                status: 404,
                headers: Vec::new(),
                body: Vec::new(),
                final_url: request.url.clone(),
                truncated: false,
            }),
        }
    }
}

fn finder(
    script: Vec<(&'static str, Reply)>,
) -> (SecurityTxtFinder<FakeFetcher>, Arc<FakeFetcher>) {
    let fetcher = Arc::new(FakeFetcher {
        script: script.into_iter().collect(),
        requests: Mutex::new(Vec::new()),
    });
    (
        SecurityTxtFinder::new(Arc::clone(&fetcher), Duration::ZERO),
        fetcher,
    )
}

fn domain() -> NormalizedDomain {
    NormalizedDomain::from_host("acme.com").unwrap()
}

const WELL_KNOWN: &str = "https://acme.com/.well-known/security.txt";
const LEGACY: &str = "https://acme.com/security.txt";

#[tokio::test]
async fn well_known_contacts_are_returned_as_security_txt_hints() {
    let (finder, fetcher) = finder(vec![(
        WELL_KNOWN,
        Ok((200, "text/plain; charset=utf-8", SIGNED)),
    )]);
    let found = finder.find(&domain(), &[], now()).await.unwrap();
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|f| f.method == ContactMethod::SecurityTxt));
    assert!(found.iter().all(|f| f.provenance.as_str() == WELL_KNOWN));
    assert_eq!(
        fetcher.requests.lock().unwrap().as_slice(),
        [(WELL_KNOWN.to_owned(), FetchPurpose::Probe)]
    );
}

#[tokio::test]
async fn legacy_location_is_the_fallback() {
    let (finder, _) = finder(vec![(
        LEGACY,
        Ok((200, "text/plain", "Contact: mailto:sec@acme.com\n")),
    )]);
    let found = finder.find(&domain(), &[], now()).await.unwrap();
    assert_eq!(found[0].address.as_str(), "sec@acme.com");
}

#[tokio::test]
async fn html_soft_404_and_expired_files_are_ignored() {
    let (finder, _) = finder(vec![
        (
            WELL_KNOWN,
            Ok((200, "text/html", "<p>Contact: mailto:a@acme.com</p>")),
        ),
        (
            LEGACY,
            Ok((
                200,
                "text/plain",
                "Contact: mailto:b@acme.com\nExpires: 2020-01-01T00:00:00Z\n",
            )),
        ),
    ]);
    assert!(finder.find(&domain(), &[], now()).await.unwrap().is_empty());
}

#[tokio::test]
async fn rate_limit_is_propagated() {
    let (finder, _) = finder(vec![(
        WELL_KNOWN,
        Err(FetchError::RateLimited {
            retry_after: Duration::from_secs(600),
        }),
    )]);
    assert!(matches!(
        finder.find(&domain(), &[], now()).await,
        Err(SourceError::RateLimited { .. })
    ));
}
