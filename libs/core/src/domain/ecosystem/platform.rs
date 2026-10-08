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

pub fn hosting_platform(domain: &NormalizedDomain) -> Option<&'static str> {
    let name = domain.as_str();
    HOSTING_PLATFORMS.into_iter().find(|platform| {
        name == *platform
            || name
                .strip_suffix(platform)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}
