use std::{collections::HashMap, sync::Arc};

use lodestone_core::{
    DnsError, EcosystemModule, Hostname, InspectionSettings, NormalizedDomain, SourceError,
};

use super::*;
use crate::ecosystem::fake::{FakeResolver, domain, target};

fn host(raw: &str) -> Hostname {
    Hostname::parse(raw).unwrap()
}

fn records(raw: &[&str]) -> Vec<String> {
    raw.iter().map(|record| (*record).to_owned()).collect()
}

#[test]
fn the_tables_are_unique_and_lowercase() {
    for (index, provider) in MX_PROVIDERS.iter().enumerate() {
        assert_eq!(provider.suffix, provider.suffix.to_ascii_lowercase());
        assert!(
            MX_PROVIDERS[index + 1..]
                .iter()
                .all(|other| other.suffix != provider.suffix),
            "{}",
            provider.suffix
        );
    }
    for table in [SPF_INCLUDES, TXT_VERIFICATIONS, CNAME_PROVIDERS] {
        for (index, (key, name)) in table.iter().enumerate() {
            assert_eq!(*key, key.to_ascii_lowercase());
            assert!(!name.is_empty());
            assert!(
                table[index + 1..].iter().all(|(other, _)| other != key),
                "{key}"
            );
        }
    }
    assert_eq!(COMMON_HOSTS.len(), 8);
}

#[test]
fn mx_hosts_name_the_mail_provider_and_the_gateway() {
    let google = mail_providers(&[host("aspmx.l.google.com"), host("alt1.aspmx.l.google.com")]);
    assert_eq!(
        google
            .iter()
            .map(|provider| provider.name)
            .collect::<Vec<_>>(),
        ["Google Workspace"]
    );
    let microsoft = mail_providers(&[host("acme-fr.mail.protection.outlook.com")]);
    assert_eq!(microsoft[0].name, "Microsoft 365");
    assert_eq!(microsoft[0].role, MailRole::Mailbox);
    let filtered = mail_providers(&[host("mx1.mail.ovh.net"), host("mx0a-001.pphosted.com")]);
    assert_eq!(
        filtered
            .iter()
            .map(|provider| (provider.name, provider.role))
            .collect::<Vec<_>>(),
        [
            ("OVHcloud", MailRole::Mailbox),
            ("Proofpoint", MailRole::Gateway)
        ]
    );
    assert!(mail_providers(&[host("mail.acme.fr")]).is_empty());
    assert!(mail_providers(&[host("notgoogle.com")]).is_empty());
}

#[test]
fn spf_includes_reveal_the_sending_services_and_the_policy() {
    let txt = records(&[
        "google-site-verification=abc",
        "v=spf1 include:_spf.google.com include:spf.mailjet.com include:12345.spf03.hubspotemail.net include:unknown.example.net ~all",
    ]);
    let spf = spf_of(&txt).unwrap();
    assert_eq!(
        spf_services(&spf),
        ["Google Workspace", "Mailjet", "HubSpot"]
    );
    assert_eq!(spf_policy(&spf), "~all (souple)");
    let strict = spf_of(&records(&[
        "v=spf1 redirect=spf.protection.outlook.com -all",
    ]))
    .unwrap();
    assert_eq!(spf_services(&strict), ["Microsoft 365"]);
    assert_eq!(spf_policy(&strict), "-all (strict)");
    assert_eq!(spf_of(&records(&["MS=ms12345"])), None);
}

#[test]
fn verification_records_reveal_tools_once() {
    let txt = records(&[
        "google-site-verification=abc",
        "google-site-verification=def",
        "MS=ms83893381",
        "atlassian-domain-verification=xyz",
        "facebook-domain-verification=1",
        "docusign=abc-123",
        "ZOOM_verify_abcdef",
        "stripe-verification=ABC",
        "apple-domain-verification=xyz",
        "hubspot-developer-verification=abc",
        "v=spf1 -all",
        "random text",
    ]);
    assert_eq!(
        verified_tools(&txt),
        [
            "Google Search Console",
            "Microsoft 365",
            "Atlassian",
            "Meta (Facebook)",
            "DocuSign",
            "Zoom",
            "Stripe",
            "Apple",
            "HubSpot"
        ]
    );
}

#[test]
fn dmarc_records_give_the_policy() {
    let dmarc = dmarc_of(&records(&[
        "v=DMARC1; p=quarantine; sp=reject; pct=50; rua=mailto:dmarc@acme.fr",
    ]))
    .unwrap();
    assert_eq!(dmarc.policy.as_deref(), Some("quarantine"));
    assert_eq!(dmarc.subdomain_policy.as_deref(), Some("reject"));
    assert_eq!(dmarc.percent, Some(50));
    assert!(dmarc.reports);
    assert_eq!(dmarc.describe(), "p=quarantine (quarantaine), 50 %");
    let reject = dmarc_of(&records(&["v=DMARC1;p=reject"])).unwrap();
    assert_eq!(reject.describe(), "p=reject (rejet)");
    assert!(!reject.reports);
    assert_eq!(dmarc_of(&records(&["v=spf1 -all"])), None);
}

#[test]
fn cname_chains_name_the_saas_provider() {
    let chain = [host("acme.statuspage.io"), host("x.stspg-customer.com")];
    let own = domain();
    let (provider, matched) = cname_provider(&chain, &own).unwrap();
    assert_eq!(provider, "Atlassian Statuspage");
    assert_eq!(matched.as_str(), "acme.statuspage.io");
    assert_eq!(
        cname_provider(&[host("acme.zendesk.com")], &own).map(|(name, _)| name),
        Some("Zendesk")
    );
    assert_eq!(
        cname_provider(&[host("d111111abcdef8.cloudfront.net")], &own).map(|(name, _)| name),
        Some("AWS CloudFront")
    );
    assert_eq!(cname_provider(&[host("lb.example.net")], &own), None);
    let clever = NormalizedDomain::from_host("clever-cloud.com").unwrap();
    let own_infra = [host("www.clever-cloud.com"), host("lb.clever-cloud.com")];
    assert_eq!(cname_provider(&own_infra, &clever), None);
}

fn resolver() -> FakeResolver {
    FakeResolver {
        mx: HashMap::from([(
            "acme.fr".to_owned(),
            Ok(vec!["acme-fr.mail.protection.outlook.com"]),
        )]),
        txt: HashMap::from([
            (
                "acme.fr".to_owned(),
                Ok(vec![
                    "v=spf1 include:spf.protection.outlook.com include:spf.sendinblue.com -all",
                    "MS=ms123",
                    "atlassian-domain-verification=abc",
                ]),
            ),
            (
                "_dmarc.acme.fr".to_owned(),
                Ok(vec!["v=DMARC1; p=reject; rua=mailto:d@acme.fr"]),
            ),
        ]),
        cname: HashMap::from([
            ("www.acme.fr".to_owned(), Ok(vec!["acme.fr"])),
            (
                "autodiscover.acme.fr".to_owned(),
                Ok(vec!["autodiscover.outlook.com"]),
            ),
            ("status.acme.fr".to_owned(), Ok(vec!["acme.statuspage.io"])),
            ("help.acme.fr".to_owned(), Ok(vec!["acme.zendesk.com"])),
            ("app.acme.fr".to_owned(), Ok(vec!["lb.example.net"])),
            ("docs.acme.fr".to_owned(), Err(DnsError::Timeout)),
        ]),
    }
}

#[tokio::test]
async fn the_module_reads_mx_spf_txt_dmarc_and_common_hosts() {
    let module = DnsEmailModule::new(Arc::new(resolver()));
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let lines: Vec<String> = findings.iter().map(|found| found.document_line()).collect();
    assert_eq!(
        lines,
        [
            "- Messagerie: Microsoft 365 (MX)",
            "- Envoi d'e-mails (SPF): Microsoft 365",
            "- Envoi d'e-mails (SPF): Brevo (Sendinblue)",
            "- SPF: -all (strict)",
            "- Outil vérifié (TXT): Microsoft 365",
            "- Outil vérifié (TXT): Atlassian",
            "- DMARC: p=reject (rejet)",
            "- Service sur sous-domaine: Microsoft 365 (autodiscover.acme.fr)",
            "- Service sur sous-domaine: Atlassian Statuspage (status.acme.fr)",
            "- Service sur sous-domaine: Zendesk (help.acme.fr)",
            "- Alias DNS: app.acme.fr → lb.example.net",
        ]
    );
    assert!(
        findings
            .iter()
            .all(|found| found.module == EcosystemModule::DnsEmail)
    );
    assert_eq!(findings[0].source_url, "dns:acme.fr?type=MX");
    assert_eq!(findings[6].source_url, "dns:_dmarc.acme.fr?type=TXT");
    assert_eq!(findings[8].source_url, "dns:status.acme.fr?type=CNAME");
    assert_eq!(findings[8].details["provider"], "Atlassian Statuspage");
}

#[tokio::test]
async fn a_domain_without_records_says_so() {
    let module = DnsEmailModule::new(Arc::new(FakeResolver::default()));
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let pairs: Vec<(&str, &str)> = findings
        .iter()
        .map(|found| (found.kind.as_str(), found.value.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("no_mx", "Aucun serveur MX"),
            ("spf", "absent"),
            ("dmarc", "absent")
        ]
    );
}

#[tokio::test]
async fn every_lookup_failing_fails_the_module() {
    let failure = || Err(DnsError::Failure("servfail".to_owned()));
    let mut resolver = FakeResolver {
        mx: HashMap::from([("acme.fr".to_owned(), failure())]),
        txt: HashMap::from([
            ("acme.fr".to_owned(), failure()),
            ("_dmarc.acme.fr".to_owned(), failure()),
        ]),
        cname: HashMap::new(),
    };
    for label in COMMON_HOSTS {
        resolver.cname.insert(format!("{label}.acme.fr"), failure());
    }
    let module = DnsEmailModule::new(Arc::new(resolver));
    let settings = InspectionSettings::default();
    let domain = domain();
    let error = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::Upstream(message) if message.contains("servfail")));
}
