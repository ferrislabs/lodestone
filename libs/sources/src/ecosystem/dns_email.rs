use std::sync::Arc;

use chrono::{DateTime, Utc};
use lodestone_core::{
    DnsError, DnsRecords, DnsResolver, EcosystemFinding, EcosystemModule, Hostname,
    InspectionTarget, NormalizedDomain, SourceError,
};
use serde_json::json;
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailRole {
    Mailbox,
    Gateway,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MxProvider {
    pub suffix: &'static str,
    pub name: &'static str,
    pub role: MailRole,
}

const fn mailbox(suffix: &'static str, name: &'static str) -> MxProvider {
    MxProvider {
        suffix,
        name,
        role: MailRole::Mailbox,
    }
}

const fn gateway(suffix: &'static str, name: &'static str) -> MxProvider {
    MxProvider {
        suffix,
        name,
        role: MailRole::Gateway,
    }
}

pub const MX_PROVIDERS: &[MxProvider] = &[
    mailbox("google.com", "Google Workspace"),
    mailbox("googlemail.com", "Google Workspace"),
    mailbox("outlook.com", "Microsoft 365"),
    mailbox("ovh.net", "OVHcloud"),
    mailbox("ovh.ca", "OVHcloud"),
    mailbox("gandi.net", "Gandi"),
    mailbox("protonmail.ch", "Proton Mail"),
    mailbox("zoho.com", "Zoho Mail"),
    mailbox("zoho.eu", "Zoho Mail"),
    mailbox("zohomail.com", "Zoho Mail"),
    mailbox("icloud.com", "iCloud Mail"),
    mailbox("ionos.fr", "IONOS"),
    mailbox("ionos.com", "IONOS"),
    mailbox("ionos.de", "IONOS"),
    mailbox("kundenserver.de", "IONOS"),
    mailbox("infomaniak.ch", "Infomaniak"),
    mailbox("secureserver.net", "GoDaddy"),
    mailbox("messagingengine.com", "Fastmail"),
    mailbox("yandex.net", "Yandex Mail"),
    mailbox("mailgun.org", "Mailgun"),
    mailbox("sendgrid.net", "SendGrid"),
    mailbox("amazonaws.com", "Amazon SES"),
    mailbox("o2switch.net", "o2switch"),
    mailbox("hostinger.com", "Hostinger"),
    mailbox("orange.fr", "Orange"),
    mailbox("free.fr", "Free"),
    mailbox("mailo.com", "Mailo"),
    mailbox("bluemind.net", "BlueMind"),
    gateway("pphosted.com", "Proofpoint"),
    gateway("ppe-hosted.com", "Proofpoint Essentials"),
    gateway("mimecast.com", "Mimecast"),
    gateway("barracudanetworks.com", "Barracuda"),
    gateway("messagelabs.com", "Symantec Email Security"),
    gateway("vadesecure.com", "Vade"),
    gateway("mailinblack.com", "Mailinblack"),
    gateway("altospam.com", "Altospam"),
    gateway("trendmicro.com", "Trend Micro"),
    gateway("trendmicro.eu", "Trend Micro"),
    gateway("iphmx.com", "Cisco Secure Email"),
    gateway("sophos.com", "Sophos"),
    gateway("hornetsecurity.com", "Hornetsecurity"),
    gateway("mailcontrol.com", "Forcepoint"),
];

pub const SPF_INCLUDES: &[(&str, &str)] = &[
    ("_spf.google.com", "Google Workspace"),
    ("spf.protection.outlook.com", "Microsoft 365"),
    ("mx.ovh.com", "OVHcloud"),
    ("spf.ovh.net", "OVHcloud"),
    ("_mailcust.gandi.net", "Gandi"),
    ("spf.protonmail.ch", "Proton Mail"),
    ("zoho.com", "Zoho Mail"),
    ("zoho.eu", "Zoho Mail"),
    ("_spf.perfora.net", "IONOS"),
    ("_spf-eu.ionos.com", "IONOS"),
    ("spf.infomaniak.ch", "Infomaniak"),
    ("spf.messagingengine.com", "Fastmail"),
    ("spf.mailjet.com", "Mailjet"),
    ("spf.sendinblue.com", "Brevo (Sendinblue)"),
    ("spf.brevo.com", "Brevo (Sendinblue)"),
    ("sendgrid.net", "SendGrid"),
    ("mailgun.org", "Mailgun"),
    ("servers.mcsv.net", "Mailchimp"),
    ("spf.mandrillapp.com", "Mailchimp Transactional (Mandrill)"),
    ("amazonses.com", "Amazon SES"),
    ("_spf.salesforce.com", "Salesforce"),
    ("exacttarget.com", "Salesforce Marketing Cloud"),
    ("hubspotemail.net", "HubSpot"),
    ("mktomail.com", "Marketo"),
    ("mail.zendesk.com", "Zendesk"),
    ("email.freshdesk.com", "Freshdesk"),
    ("helpscoutemail.com", "Help Scout"),
    ("_spf.atlassian.net", "Atlassian"),
    ("spf.mtasv.net", "Postmark"),
    ("sparkpostmail.com", "SparkPost"),
    ("_spf.elasticemail.com", "Elastic Email"),
    ("_spf.mlsend.com", "MailerLite"),
    ("pphosted.com", "Proofpoint"),
    ("mimecast.com", "Mimecast"),
    ("_spf.intercom.io", "Intercom"),
    ("mail.intercom.io", "Intercom"),
    ("_spf.docusign.net", "DocuSign"),
    ("customeriomail.com", "Customer.io"),
    ("_spf.lemlist.com", "lemlist"),
    ("sellsy.com", "Sellsy"),
    ("spf.sarbacane.com", "Sarbacane"),
];

pub const TXT_VERIFICATIONS: &[(&str, &str)] = &[
    ("google-site-verification=", "Google Search Console"),
    ("ms=", "Microsoft 365"),
    ("atlassian-domain-verification=", "Atlassian"),
    ("facebook-domain-verification=", "Meta (Facebook)"),
    ("hubspot-developer-verification=", "HubSpot"),
    ("hubspot-domain-verification=", "HubSpot"),
    ("hs-site-verification=", "HubSpot"),
    ("salesforce-domain-verification=", "Salesforce"),
    ("pardot", "Salesforce Pardot"),
    ("docusign=", "DocuSign"),
    ("zoom_verify_", "Zoom"),
    ("stripe-verification=", "Stripe"),
    ("apple-domain-verification=", "Apple"),
    ("adobe-idp-site-verification=", "Adobe"),
    ("adobe-sign-verification=", "Adobe Acrobat Sign"),
    ("slack-domain-verification=", "Slack"),
    ("miro-verification=", "Miro"),
    ("dropbox-domain-verification=", "Dropbox"),
    ("onetrust-domain-verification=", "OneTrust"),
    ("canva-site-verification=", "Canva"),
    ("cisco-ci-domain-verification=", "Cisco Webex"),
    ("webexdomainverification", "Cisco Webex"),
    ("yandex-verification:", "Yandex"),
    ("brevo-code:", "Brevo (Sendinblue)"),
    ("sendinblue-code:", "Brevo (Sendinblue)"),
    ("mailjet-domain-validation", "Mailjet"),
    ("have-i-been-pwned-verification=", "Have I Been Pwned"),
    ("logmein-verification-code=", "LogMeIn"),
    ("citrix-verification-code=", "Citrix"),
    ("teamviewer-sso-verification=", "TeamViewer"),
    ("jamf-site-verification=", "Jamf"),
    ("knowbe4-site-verification=", "KnowBe4"),
    ("openai-domain-verification=", "OpenAI"),
    ("anthropic-domain-verification", "Anthropic"),
    ("workplace-domain-verification=", "Workplace from Meta"),
    ("zapier-domain-verification-challenge=", "Zapier"),
    ("smartsheet-site-validation=", "Smartsheet"),
    ("loom-site-verification=", "Loom"),
    ("segment-site-verification=", "Segment"),
    ("pinterest-site-verification=", "Pinterest"),
    ("twilio-domain-verification=", "Twilio"),
    ("mongodb-site-verification=", "MongoDB Atlas"),
    ("postman-domain-verification=", "Postman"),
    ("box-domain-verification=", "Box"),
    ("amazonses:", "Amazon SES"),
    ("globalsign-domain-verification=", "GlobalSign"),
    ("gitlab-pages-verification-code=", "GitLab"),
    ("notion-domain-verification=", "Notion"),
    ("figma-domain-verification=", "Figma"),
    ("lastpass-verification-code=", "LastPass"),
    ("1password-site-verification=", "1Password"),
    ("okta-verification=", "Okta"),
    ("intercom-domain-validation=", "Intercom"),
    ("klaviyo-site-verification=", "Klaviyo"),
    ("wrike-verification=", "Wrike"),
];

pub const COMMON_HOSTS: [&str; 8] = [
    "www",
    "mail",
    "autodiscover",
    "status",
    "help",
    "support",
    "docs",
    "app",
];

pub const CNAME_PROVIDERS: &[(&str, &str)] = &[
    ("autodiscover.outlook.com", "Microsoft 365"),
    ("ghs.googlehosted.com", "Google"),
    ("ghs.google.com", "Google"),
    ("cdn.cloudflare.net", "Cloudflare"),
    ("pages.dev", "Cloudflare Pages"),
    ("fastly.net", "Fastly"),
    ("fastlylb.net", "Fastly"),
    ("edgekey.net", "Akamai"),
    ("edgesuite.net", "Akamai"),
    ("akamaiedge.net", "Akamai"),
    ("cloudfront.net", "AWS CloudFront"),
    ("elb.amazonaws.com", "AWS Elastic Load Balancing"),
    ("amazonaws.com", "Amazon Web Services"),
    ("azurewebsites.net", "Azure App Service"),
    ("azureedge.net", "Azure CDN"),
    ("azurefd.net", "Azure Front Door"),
    ("trafficmanager.net", "Azure Traffic Manager"),
    ("cloudapp.azure.com", "Microsoft Azure"),
    ("vercel-dns.com", "Vercel"),
    ("vercel.app", "Vercel"),
    ("netlify.app", "Netlify"),
    ("netlify.com", "Netlify"),
    ("github.io", "GitHub Pages"),
    ("herokudns.com", "Heroku"),
    ("herokuapp.com", "Heroku"),
    ("onrender.com", "Render"),
    ("fly.dev", "Fly.io"),
    ("firebaseapp.com", "Firebase Hosting"),
    ("web.app", "Firebase Hosting"),
    ("wpengine.com", "WP Engine"),
    ("kinsta.cloud", "Kinsta"),
    ("pantheonsite.io", "Pantheon"),
    ("webflow.io", "Webflow"),
    ("proxy-ssl.webflow.com", "Webflow"),
    ("myshopify.com", "Shopify"),
    ("shops.myshopify.com", "Shopify"),
    ("squarespace.com", "Squarespace"),
    ("wixdns.net", "Wix"),
    ("framer.app", "Framer"),
    ("unbouncepages.com", "Unbounce"),
    ("hubspot.net", "HubSpot"),
    ("hs-sites.com", "HubSpot"),
    ("hubspotpagebuilder.com", "HubSpot"),
    ("mktoweb.com", "Marketo"),
    ("mktossl.com", "Marketo"),
    ("pardot.com", "Salesforce Pardot"),
    ("force.com", "Salesforce"),
    ("salesforce.com", "Salesforce"),
    ("zendesk.com", "Zendesk"),
    ("freshdesk.com", "Freshdesk"),
    ("helpscoutdocs.com", "Help Scout"),
    ("intercom.help", "Intercom"),
    ("crisp.help", "Crisp"),
    ("uservoice.com", "UserVoice"),
    ("statuspage.io", "Atlassian Statuspage"),
    ("stspg-customer.com", "Atlassian Statuspage"),
    ("atlassian.net", "Atlassian"),
    ("betteruptime.com", "Better Stack"),
    ("instatus.com", "Instatus"),
    ("readme.io", "ReadMe"),
    ("gitbook.io", "GitBook"),
    ("document360.io", "Document360"),
    ("sendgrid.net", "SendGrid"),
    ("mailgun.org", "Mailgun"),
    ("ovh.net", "OVHcloud"),
    ("gandi.net", "Gandi"),
    ("scw.cloud", "Scaleway"),
    ("scalingo.io", "Scalingo"),
    ("cleverapps.io", "Clever Cloud"),
    ("clever-cloud.com", "Clever Cloud"),
    ("platform.sh", "Platform.sh"),
    ("recruitee.com", "Recruitee"),
    ("b-cdn.net", "Bunny CDN"),
    ("siteforce.com", "Salesforce Experience Cloud"),
    ("teamtailor.com", "Teamtailor"),
];

pub fn hop_matches(hop: &str, suffix: &str) -> bool {
    hop == suffix
        || hop
            .strip_suffix(suffix)
            .is_some_and(|rest| rest.ends_with('.'))
}

pub fn mail_providers(mx: &[Hostname]) -> Vec<&'static MxProvider> {
    let mut providers: Vec<&'static MxProvider> = Vec::new();
    for host in mx {
        if let Some(provider) = MX_PROVIDERS
            .iter()
            .find(|provider| hop_matches(host.as_str(), provider.suffix))
            && !providers.iter().any(|known| known.name == provider.name)
        {
            providers.push(provider);
        }
    }
    providers
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spf {
    pub includes: Vec<String>,
    pub all: Option<String>,
}

pub fn spf_of(records: &[String]) -> Option<Spf> {
    let record = records
        .iter()
        .find(|record| record.trim().to_ascii_lowercase().starts_with("v=spf1"))?;
    let mut includes = Vec::new();
    let mut all = None;
    for term in record.split_whitespace().skip(1) {
        let lowered = term.to_ascii_lowercase();
        if let Some(target) = lowered
            .strip_prefix("include:")
            .or_else(|| lowered.strip_prefix("redirect="))
        {
            includes.push(target.trim_end_matches('.').to_owned());
        } else if lowered.ends_with("all") && lowered.len() <= 4 {
            all = Some(lowered);
        }
    }
    Some(Spf { includes, all })
}

pub fn spf_services(spf: &Spf) -> Vec<&'static str> {
    let mut services: Vec<&'static str> = Vec::new();
    for include in &spf.includes {
        if let Some((_, name)) = SPF_INCLUDES
            .iter()
            .find(|(suffix, _)| hop_matches(include, suffix))
            && !services.contains(name)
        {
            services.push(name);
        }
    }
    services
}

pub fn spf_policy(spf: &Spf) -> String {
    match spf.all.as_deref() {
        Some("-all") => "-all (strict)".to_owned(),
        Some("~all") => "~all (souple)".to_owned(),
        Some("?all") => "?all (neutre)".to_owned(),
        Some("+all" | "all") => "+all (permissif)".to_owned(),
        _ => "sans règle finale".to_owned(),
    }
}

pub fn verified_tools(records: &[String]) -> Vec<&'static str> {
    let mut tools: Vec<&'static str> = Vec::new();
    for record in records {
        let lowered = record.trim().to_ascii_lowercase();
        if let Some((_, tool)) = TXT_VERIFICATIONS
            .iter()
            .find(|(prefix, _)| lowered.starts_with(prefix))
            && !tools.contains(tool)
        {
            tools.push(tool);
        }
    }
    tools
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dmarc {
    pub policy: Option<String>,
    pub subdomain_policy: Option<String>,
    pub percent: Option<u8>,
    pub reports: bool,
}

pub fn dmarc_of(records: &[String]) -> Option<Dmarc> {
    let record = records
        .iter()
        .find(|record| record.trim().to_ascii_lowercase().starts_with("v=dmarc1"))?;
    let mut dmarc = Dmarc {
        policy: None,
        subdomain_policy: None,
        percent: None,
        reports: false,
    };
    for tag in record.split(';') {
        let Some((key, value)) = tag.split_once('=') else {
            continue;
        };
        let value = value.trim().to_ascii_lowercase();
        match key.trim().to_ascii_lowercase().as_str() {
            "p" => dmarc.policy = Some(value),
            "sp" => dmarc.subdomain_policy = Some(value),
            "pct" => dmarc.percent = value.parse().ok(),
            "rua" | "ruf" => dmarc.reports = dmarc.reports || !value.is_empty(),
            _ => {}
        }
    }
    Some(dmarc)
}

impl Dmarc {
    pub fn describe(&self) -> String {
        let mut text = match self.policy.as_deref() {
            Some("reject") => "p=reject (rejet)".to_owned(),
            Some("quarantine") => "p=quarantine (quarantaine)".to_owned(),
            Some("none") => "p=none (surveillance)".to_owned(),
            Some(other) => format!("p={other}"),
            None => "sans politique".to_owned(),
        };
        if let Some(percent) = self.percent.filter(|percent| *percent < 100) {
            text.push_str(&format!(", {percent} %"));
        }
        text
    }
}

pub fn cname_provider<'a>(
    chain: &'a [Hostname],
    own: &NormalizedDomain,
) -> Option<(&'static str, &'a Hostname)> {
    chain
        .iter()
        .filter(|hop| hop.registrable_domain().as_ref() != Some(own))
        .find_map(|hop| {
            CNAME_PROVIDERS
                .iter()
                .find(|(suffix, _)| hop_matches(hop.as_str(), suffix))
                .map(|(_, name)| (*name, hop))
        })
}

struct Lookups {
    attempted: usize,
    failed: usize,
    last_error: Option<DnsError>,
}

impl Lookups {
    fn record<T: Default>(&mut self, result: Result<T, DnsError>, what: &str) -> T {
        self.attempted += 1;
        match result {
            Ok(value) => value,
            Err(error) => {
                debug!(what, %error, "dns lookup failed");
                self.failed += 1;
                self.last_error = Some(error);
                T::default()
            }
        }
    }
}

pub struct DnsEmailModule<R> {
    resolver: Arc<R>,
}

fn mx_findings(
    domain: &NormalizedDomain,
    mx: &[Hostname],
    now: DateTime<Utc>,
) -> Vec<EcosystemFinding> {
    let module = EcosystemModule::DnsEmail;
    let source = format!("dns:{domain}?type=MX");
    let hosts: Vec<&str> = mx.iter().map(Hostname::as_str).collect();
    let details = json!({"mx": hosts});
    let Some(first) = hosts.first() else {
        return vec![
            EcosystemFinding::new(
                module,
                "no_mx",
                "Messagerie",
                "Aucun serveur MX",
                source.as_str(),
                now,
            )
            .keyed("no_mx"),
        ];
    };
    let providers = mail_providers(mx);
    if providers.is_empty() {
        return vec![
            EcosystemFinding::new(
                module,
                "mail_server",
                "Serveur de messagerie",
                *first,
                source.as_str(),
                now,
            )
            .keyed("mail_server")
            .with_details(details),
        ];
    }
    providers
        .into_iter()
        .map(|provider| {
            let (kind, label) = match provider.role {
                MailRole::Mailbox => ("mail_provider", "Messagerie"),
                MailRole::Gateway => ("mail_gateway", "Filtrage des e-mails"),
            };
            EcosystemFinding::new(
                module,
                kind,
                label,
                format!("{} (MX)", provider.name),
                source.as_str(),
                now,
            )
            .keyed(format!("{kind}:{}", provider.name))
            .with_details(details.clone())
        })
        .collect()
}

fn txt_findings(
    domain: &NormalizedDomain,
    records: &[String],
    now: DateTime<Utc>,
) -> Vec<EcosystemFinding> {
    let module = EcosystemModule::DnsEmail;
    let source = format!("dns:{domain}?type=TXT");
    let mut findings = Vec::new();
    match spf_of(records) {
        Some(spf) => {
            for service in spf_services(&spf) {
                findings.push(
                    EcosystemFinding::new(
                        module,
                        "spf_service",
                        "Envoi d'e-mails (SPF)",
                        service,
                        source.as_str(),
                        now,
                    )
                    .with_details(json!({"includes": spf.includes})),
                );
            }
            findings.push(
                EcosystemFinding::new(module, "spf", "SPF", spf_policy(&spf), source.as_str(), now)
                    .keyed("spf")
                    .with_details(json!({"includes": spf.includes, "all": spf.all})),
            );
        }
        None => findings.push(
            EcosystemFinding::new(module, "spf", "SPF", "absent", source.as_str(), now)
                .keyed("spf"),
        ),
    }
    findings.extend(verified_tools(records).into_iter().map(|tool| {
        EcosystemFinding::new(
            module,
            "verified_tool",
            "Outil vérifié (TXT)",
            tool,
            source.as_str(),
            now,
        )
    }));
    findings
}

fn dmarc_finding(
    domain: &NormalizedDomain,
    records: &[String],
    now: DateTime<Utc>,
) -> EcosystemFinding {
    let source = format!("dns:_dmarc.{domain}?type=TXT");
    let finding = |value: String| {
        EcosystemFinding::new(
            EcosystemModule::DnsEmail,
            "dmarc",
            "DMARC",
            value,
            source.as_str(),
            now,
        )
        .keyed("dmarc")
    };
    match dmarc_of(records) {
        Some(dmarc) => finding(dmarc.describe()).with_details(json!({
            "policy": dmarc.policy,
            "subdomain_policy": dmarc.subdomain_policy,
            "percent": dmarc.percent,
            "reports": dmarc.reports,
        })),
        None => finding("absent".to_owned()),
    }
}

fn cname_finding(
    domain: &NormalizedDomain,
    host: &Hostname,
    chain: &[Hostname],
    now: DateTime<Utc>,
) -> Option<EcosystemFinding> {
    let module = EcosystemModule::DnsEmail;
    let source = format!("dns:{host}?type=CNAME");
    let hops: Vec<&str> = chain.iter().map(Hostname::as_str).collect();
    if let Some((provider, matched)) = cname_provider(chain, domain) {
        return Some(
            EcosystemFinding::new(
                module,
                "saas_host",
                "Service sur sous-domaine",
                format!("{provider} ({host})"),
                source.as_str(),
                now,
            )
            .keyed(format!("saas_host:{host}"))
            .with_details(json!({
                "host": host.as_str(),
                "provider": provider,
                "chain": hops,
                "matched": matched.as_str(),
            })),
        );
    }
    let last = chain.last()?;
    if last.registrable_domain().as_ref() == Some(domain) {
        return None;
    }
    Some(
        EcosystemFinding::new(
            module,
            "cname",
            "Alias DNS",
            format!("{host} → {last}"),
            source.as_str(),
            now,
        )
        .keyed(format!("cname:{host}"))
        .with_details(json!({"host": host.as_str(), "chain": hops})),
    )
}

impl<R: DnsRecords + DnsResolver> DnsEmailModule<R> {
    pub fn new(resolver: Arc<R>) -> Self {
        Self { resolver }
    }

    pub async fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> Result<Vec<EcosystemFinding>, SourceError> {
        let domain = target.domain;
        let now = target.now;
        let mut lookups = Lookups {
            attempted: 0,
            failed: 0,
            last_error: None,
        };
        let mx_result = self.resolver.mx_hosts(domain).await;
        let mx_failed = mx_result.is_err();
        let mx = lookups.record(mx_result, "mx");
        let txt_result = self.resolver.txt_records(domain.as_str()).await;
        let txt_failed = txt_result.is_err();
        let txt = lookups.record(txt_result, "txt");
        let dmarc_result = self.resolver.txt_records(&format!("_dmarc.{domain}")).await;
        let dmarc_failed = dmarc_result.is_err();
        let dmarc = lookups.record(dmarc_result, "dmarc");
        let mut findings = Vec::new();
        if !mx_failed {
            findings.extend(mx_findings(domain, &mx, now));
        }
        if !txt_failed {
            findings.extend(txt_findings(domain, &txt, now));
        }
        if !dmarc_failed {
            findings.push(dmarc_finding(domain, &dmarc, now));
        }
        for label in COMMON_HOSTS {
            let Ok(host) = Hostname::parse(&format!("{label}.{domain}")) else {
                continue;
            };
            let chain = lookups.record(self.resolver.cname_chain(&host).await, "cname");
            findings.extend(cname_finding(domain, &host, &chain, now));
        }
        if lookups.failed == lookups.attempted
            && let Some(error) = lookups.last_error
        {
            return Err(SourceError::Upstream(format!(
                "every DNS lookup failed: {error}"
            )));
        }
        Ok(findings)
    }
}

#[cfg(test)]
mod tests;
