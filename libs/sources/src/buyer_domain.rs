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

const JOB_BOARD_SUFFIXES: [&str; 31] = [
    "indeed.com",
    "indeed.fr",
    "linkedin.com",
    "welcometothejungle.com",
    "hellowork.com",
    "regionsjob.com",
    "meteojob.com",
    "monster.fr",
    "monster.com",
    "apec.fr",
    "jobteaser.com",
    "glassdoor.fr",
    "glassdoor.com",
    "cadremploi.fr",
    "jobijoba.com",
    "talent.com",
    "jooble.org",
    "adzuna.fr",
    "pole-emploi.fr",
    "francetravail.fr",
    "francetravail.io",
    "randstad.fr",
    "adecco.fr",
    "manpower.fr",
    "synergie.fr",
    "crit-job.com",
    "proman-emploi.fr",
    "page-personnel.fr",
    "michaelpage.fr",
    "hays.fr",
    "walters-people.fr",
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

fn is_job_board(host: &str) -> bool {
    JOB_BOARD_SUFFIXES
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
    if is_platform(&host) || is_job_board(&host) {
        return None;
    }
    let domain = NormalizedDomain::from_host(&host).ok()?;
    if is_platform(domain.as_str()) || is_job_board(domain.as_str()) || is_webmail(domain.as_str())
    {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_boards_and_staffing_agencies_are_never_a_buyer() {
        for host in [
            "indeed.com",
            "fr.indeed.com",
            "www.linkedin.com",
            "welcometothejungle.com",
            "randstad.fr",
            "careers.adecco.fr",
        ] {
            assert!(buyer_domain(host).is_none(), "{host}");
        }
        assert!(domain_of_url("https://www.apec.fr/offre").is_none());
        assert!(domain_of_email("rh@hellowork.com").is_none());
    }

    #[test]
    fn lookalike_hosts_are_kept() {
        assert!(buyer_domain("notindeed.com").is_some());
        assert_eq!(buyer_domain("www.acme.fr").unwrap().as_str(), "acme.fr");
    }
}
