use lodestone_core::NormalizedDomain;
use url::Url;

const PLATFORM_SUFFIXES: [&str; 22] = [
    "aws-france.com",
    "achatpublic.com",
    "marches-publics.gouv.fr",
    "marches-securises.fr",
    "marches-publics.info",
    "e-marchespublics.com",
    "megalis.bretagne.bzh",
    "maximilien.fr",
    "klekoon.com",
    "synapse-entreprises.com",
    "centraledesmarches.com",
    "dematis.com",
    "omnikles.com",
    "ternum-bfc.fr",
    "alsacemarchespublics.eu",
    "marchespublics.grandest.fr",
    "e-bourgogne.fr",
    "telerecours.fr",
    "mpe76.fr",
    "demat-ampa.fr",
    "marchespublics-ght.fr",
    "marches-publics-securises.fr",
];

const WEBMAIL_DOMAINS: [&str; 14] = [
    "gmail.com",
    "orange.fr",
    "wanadoo.fr",
    "free.fr",
    "laposte.net",
    "sfr.fr",
    "neuf.fr",
    "bbox.fr",
    "icloud.com",
    "aol.com",
    "protonmail.com",
    "proton.me",
    "me.com",
    "msn.com",
];

const WEBMAIL_LABELS: [&str; 4] = ["hotmail", "outlook", "yahoo", "live"];

fn matches_suffix(host: &str, suffix: &str) -> bool {
    host == suffix
        || host
            .strip_suffix(suffix)
            .is_some_and(|rest| rest.ends_with('.'))
}

fn is_platform(host: &str) -> bool {
    PLATFORM_SUFFIXES
        .iter()
        .any(|suffix| matches_suffix(host, suffix))
}

fn is_webmail(domain: &str) -> bool {
    WEBMAIL_DOMAINS.contains(&domain)
        || domain
            .split('.')
            .next()
            .is_some_and(|label| WEBMAIL_LABELS.contains(&label))
}

pub(crate) fn buyer_domain(host: &str) -> Option<NormalizedDomain> {
    let host = host.trim().to_lowercase();
    if is_platform(&host) {
        return None;
    }
    let domain = NormalizedDomain::from_host(&host).ok()?;
    if is_platform(domain.as_str()) || is_webmail(domain.as_str()) {
        return None;
    }
    Some(domain)
}

pub(crate) fn domain_of_url(raw: &str) -> Option<NormalizedDomain> {
    let raw = raw.trim();
    let url = Url::parse(raw)
        .ok()
        .filter(|url| url.host_str().is_some())
        .or_else(|| Url::parse(&format!("https://{raw}")).ok())?;
    buyer_domain(url.host_str()?)
}

pub(crate) fn domain_of_email(raw: &str) -> Option<NormalizedDomain> {
    let (_, host) = raw.trim().rsplit_once('@')?;
    buyer_domain(host)
}
