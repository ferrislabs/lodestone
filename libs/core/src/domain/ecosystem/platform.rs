use crate::domain::NormalizedDomain;

pub const HOSTING_PLATFORMS: [&str; 18] = [
    "notion.site",
    "github.io",
    "gitlab.io",
    "vercel.app",
    "netlify.app",
    "pages.dev",
    "webflow.io",
    "wixsite.com",
    "framer.website",
    "carrd.co",
    "herokuapp.com",
    "azurewebsites.net",
    "web.app",
    "firebaseapp.com",
    "blogspot.com",
    "wordpress.com",
    "substack.com",
    "myshopify.com",
];

pub const SAAS_TENANT_DOMAINS: [&str; 20] = [
    "crm.dev",
    "force.com",
    "salesforce.com",
    "my.salesforce.com",
    "okta.com",
    "oktapreview.com",
    "okta-emea.com",
    "auth0.com",
    "b2clogin.com",
    "ciamlogin.com",
    "onmicrosoft.com",
    "sharepoint.com",
    "zendesk.com",
    "freshdesk.com",
    "atlassian.net",
    "myshopify.com",
    "cloudfront.net",
    "azurefd.net",
    "herokudns.com",
    "amazonaws.com",
];

pub const SAAS_TENANT_REASON: &str = "domaine d'un fournisseur SaaS";

fn under<'a>(name: &str, suffixes: &[&'a str]) -> Option<&'a str> {
    suffixes.iter().copied().find(|suffix| {
        name == *suffix
            || name
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

pub fn hosting_platform(domain: &NormalizedDomain) -> Option<&'static str> {
    under(domain.as_str(), &HOSTING_PLATFORMS)
}

pub fn saas_tenant(name: &str) -> Option<&'static str> {
    under(name, &SAAS_TENANT_DOMAINS)
}
