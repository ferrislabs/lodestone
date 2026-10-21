use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

use self::{
    Matcher::{Asset, Cookie, Header, Html, Meta},
    TechCategory::{
        Analytics, Auth, Chat, Cms, CrmMarketing, Ecommerce, Framework, HostingCdn, Other, Payment,
    },
};

const MAX_SCANNED_HTML: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TechCategory {
    Cms,
    Framework,
    HostingCdn,
    Analytics,
    Chat,
    CrmMarketing,
    Ecommerce,
    Auth,
    Payment,
    Other,
}

impl TechCategory {
    pub const ALL: [Self; 10] = [
        Self::Cms,
        Self::Framework,
        Self::HostingCdn,
        Self::Analytics,
        Self::Chat,
        Self::CrmMarketing,
        Self::Ecommerce,
        Self::Auth,
        Self::Payment,
        Self::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cms => "cms",
            Self::Framework => "framework",
            Self::HostingCdn => "hosting_cdn",
            Self::Analytics => "analytics",
            Self::Chat => "chat",
            Self::CrmMarketing => "crm_marketing",
            Self::Ecommerce => "ecommerce",
            Self::Auth => "auth",
            Self::Payment => "payment",
            Self::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cms => "CMS",
            Self::Framework => "Framework",
            Self::HostingCdn => "Hébergement et CDN",
            Self::Analytics => "Mesure d'audience",
            Self::Chat => "Chat et support",
            Self::CrmMarketing => "CRM et marketing",
            Self::Ecommerce => "E-commerce",
            Self::Auth => "Authentification",
            Self::Payment => "Paiement",
            Self::Other => "Autre",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matcher {
    Header(&'static str, &'static str),
    Meta(&'static str, &'static str),
    Asset(&'static str),
    Cookie(&'static str),
    Html(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TechSignature {
    pub id: &'static str,
    pub name: &'static str,
    pub category: TechCategory,
    pub matchers: &'static [Matcher],
}

const fn tech(
    id: &'static str,
    name: &'static str,
    category: TechCategory,
    matchers: &'static [Matcher],
) -> TechSignature {
    TechSignature {
        id,
        name,
        category,
        matchers,
    }
}

pub const TECH_SIGNATURES: &[TechSignature] = &[
    tech(
        "wordpress",
        "WordPress",
        Cms,
        &[
            Meta("generator", r"^WordPress\s*([\d.]+)?"),
            Asset(r"/wp-(?:content|includes)/"),
            Html(r#"<link[^>]+rel=["']https://api\.w\.org/"#),
        ],
    ),
    tech(
        "drupal",
        "Drupal",
        Cms,
        &[
            Meta("generator", r"^Drupal\s*(\d+)?"),
            Header("x-generator", r"^Drupal\s*(\d+)?"),
            Header("x-drupal-cache", ""),
            Asset(r"/sites/(?:default|all)/(?:files|themes|modules)/|/core/misc/drupal"),
            Html(r#"data-drupal-selector|Drupal\.settings"#),
        ],
    ),
    tech(
        "joomla",
        "Joomla",
        Cms,
        &[
            Meta(
                "generator",
                r"^Joomla!?(?:\s*-\s*Open Source Content Management)?\s*([\d.]+)?",
            ),
            Asset(r"/media/(?:jui|system)/js/"),
        ],
    ),
    tech(
        "typo3",
        "TYPO3",
        Cms,
        &[
            Meta("generator", r"^TYPO3\s*(?:CMS\s*)?([\d.]+)?"),
            Asset(r"/typo3(?:conf|temp)/"),
        ],
    ),
    tech(
        "spip",
        "SPIP",
        Cms,
        &[
            Meta("generator", r"^SPIP\s*([\d.]+)?"),
            Header("composed-by", r"SPIP\s*([\d.]+)?"),
        ],
    ),
    tech(
        "ghost",
        "Ghost",
        Cms,
        &[Meta("generator", r"^Ghost\s*([\d.]+)?")],
    ),
    tech(
        "wix",
        "Wix",
        Cms,
        &[
            Meta("generator", r"^Wix\.com"),
            Header("x-wix-request-id", ""),
            Asset(r"static\.(?:wixstatic|parastorage)\.com"),
        ],
    ),
    tech(
        "webflow",
        "Webflow",
        Cms,
        &[
            Meta("generator", r"^Webflow"),
            Html(r"data-wf-(?:page|site)="),
            Asset(r"(?:assets|uploads-ssl)\.website-files\.com|cdn\.prod\.website-files\.com"),
        ],
    ),
    tech(
        "squarespace",
        "Squarespace",
        Cms,
        &[
            Html(r"Static\.SQUARESPACE_CONTEXT"),
            Asset(r"static1\.squarespace\.com|assets\.squarespace\.com"),
        ],
    ),
    tech(
        "framer",
        "Framer",
        Cms,
        &[
            Meta("generator", r"^Framer\s*([\w.]+)?"),
            Asset(r"framerusercontent\.com|events\.framer\.com"),
        ],
    ),
    tech(
        "hubspot_cms",
        "HubSpot CMS",
        Cms,
        &[Meta("generator", r"^HubSpot"), Header("x-hs-hub-id", "")],
    ),
    tech(
        "aem",
        "Adobe Experience Manager",
        Cms,
        &[Asset(r"/etc\.clientlibs/|/etc/designs/")],
    ),
    tech(
        "sitecore",
        "Sitecore",
        Cms,
        &[Asset(r"/-/media/"), Cookie(r"^SC_ANALYTICS_GLOBAL_COOKIE$")],
    ),
    tech(
        "hugo",
        "Hugo",
        Cms,
        &[Meta("generator", r"^Hugo\s*([\d.]+)?")],
    ),
    tech(
        "jekyll",
        "Jekyll",
        Cms,
        &[Meta("generator", r"^Jekyll\s*v?([\d.]+)?")],
    ),
    tech(
        "odoo",
        "Odoo",
        Cms,
        &[Meta("generator", r"^Odoo"), Asset(r"/web/assets/\d+")],
    ),
    tech(
        "jimdo",
        "Jimdo",
        Cms,
        &[Asset(r"jimdo(?:cdn|static)?\.com")],
    ),
    tech(
        "nextjs",
        "Next.js",
        Framework,
        &[
            Header("x-powered-by", r"^Next\.js\s*([\d.]+)?"),
            Html(r"__NEXT_DATA__|/_next/static/"),
        ],
    ),
    tech(
        "nuxt",
        "Nuxt",
        Framework,
        &[Html(r"__NUXT__|/_nuxt/|window\.__NUXT")],
    ),
    tech(
        "gatsby",
        "Gatsby",
        Framework,
        &[
            Meta("generator", r"^Gatsby\s*([\d.]+)?"),
            Html(r#"id=["']___gatsby["']"#),
        ],
    ),
    tech(
        "astro",
        "Astro",
        Framework,
        &[
            Meta("generator", r"^Astro\s*v?([\d.]+)?"),
            Html(r"<astro-island"),
        ],
    ),
    tech(
        "sveltekit",
        "SvelteKit",
        Framework,
        &[Html(r"__sveltekit_|data-sveltekit-")],
    ),
    tech("remix", "Remix", Framework, &[Html(r"__remixContext")]),
    tech(
        "react",
        "React",
        Framework,
        &[
            Html(r"data-reactroot|data-reactid"),
            Asset(r"react(?:-dom)?(?:\.production)?(?:\.min)?\.js"),
        ],
    ),
    tech(
        "vue",
        "Vue.js",
        Framework,
        &[
            Html(r#"data-v-[0-9a-f]{8}|data-server-rendered=["']true"#),
            Asset(r"vue(?:\.global)?(?:\.prod)?(?:\.min)?\.js"),
        ],
    ),
    tech(
        "angular",
        "Angular",
        Framework,
        &[Html(r#"ng-version=["']([\d.]+)"#)],
    ),
    tech(
        "laravel",
        "Laravel",
        Framework,
        &[Cookie(r"^laravel_session$")],
    ),
    tech(
        "symfony",
        "Symfony",
        Framework,
        &[Header("x-debug-token", ""), Cookie(r"^sf_redirect$")],
    ),
    tech(
        "django",
        "Django",
        Framework,
        &[Html(r"csrfmiddlewaretoken"), Cookie(r"^django_language$")],
    ),
    tech(
        "rails",
        "Ruby on Rails",
        Framework,
        &[Meta("csrf-param", r"^authenticity_token$")],
    ),
    tech(
        "aspnet",
        "ASP.NET",
        Framework,
        &[
            Header("x-aspnet-version", r"([\d.]+)"),
            Header("x-powered-by", r"^ASP\.NET"),
            Cookie(r"^ASP\.NET_SessionId$|^\.AspNetCore\."),
            Html(r#"name=["']__VIEWSTATE["']"#),
        ],
    ),
    tech(
        "php",
        "PHP",
        Framework,
        &[
            Header("x-powered-by", r"^PHP/?([\d.]+)?"),
            Cookie(r"^PHPSESSID$"),
        ],
    ),
    tech(
        "express",
        "Express",
        Framework,
        &[Header("x-powered-by", r"^Express$")],
    ),
    tech(
        "java_servlet",
        "Java (servlet)",
        Framework,
        &[Cookie(r"^JSESSIONID$")],
    ),
    tech(
        "jquery",
        "jQuery",
        Framework,
        &[Asset(
            r"jquery[.-]?(\d+(?:\.\d+)+)?(?:\.slim)?(?:\.min)?\.js",
        )],
    ),
    tech(
        "bootstrap",
        "Bootstrap",
        Framework,
        &[Asset(
            r"bootstrap(?:@(\d+(?:\.\d+)+))?(?:/[^?#]*)?(?:\.bundle)?(?:\.min)?\.(?:css|js)",
        )],
    ),
    tech(
        "cloudflare",
        "Cloudflare",
        HostingCdn,
        &[
            Header("cf-ray", ""),
            Header("server", r"^cloudflare"),
            Cookie(r"^__cf_bm$|^__cfruid$"),
        ],
    ),
    tech(
        "fastly",
        "Fastly",
        HostingCdn,
        &[
            Header("x-fastly-request-id", ""),
            Header("fastly-debug-digest", ""),
            Header("x-served-by", r"^cache-[a-z]{3}"),
        ],
    ),
    tech(
        "akamai",
        "Akamai",
        HostingCdn,
        &[
            Header("x-akamai-transformed", ""),
            Header("akamai-grn", ""),
            Header("server", r"^AkamaiGHost|^AkamaiNetStorage"),
        ],
    ),
    tech(
        "vercel",
        "Vercel",
        HostingCdn,
        &[Header("x-vercel-id", ""), Header("server", r"^Vercel")],
    ),
    tech(
        "netlify",
        "Netlify",
        HostingCdn,
        &[Header("x-nf-request-id", ""), Header("server", r"^Netlify")],
    ),
    tech(
        "cloudfront",
        "AWS CloudFront",
        HostingCdn,
        &[Header("x-amz-cf-id", ""), Header("via", r"CloudFront")],
    ),
    tech(
        "amazon_s3",
        "Amazon S3",
        HostingCdn,
        &[
            Header("server", r"^AmazonS3"),
            Header("x-amz-request-id", ""),
        ],
    ),
    tech(
        "ovhcloud",
        "OVHcloud",
        HostingCdn,
        &[
            Header("x-iplb-instance", ""),
            Header("x-iplb-request-id", ""),
            Header("x-ovh-request-id", ""),
        ],
    ),
    tech(
        "google_cloud",
        "Google Cloud",
        HostingCdn,
        &[
            Header("via", r"1\.1 google"),
            Header("server", r"^Google Frontend"),
        ],
    ),
    tech(
        "azure",
        "Microsoft Azure",
        HostingCdn,
        &[
            Header("x-azure-ref", ""),
            Header("x-msedge-ref", ""),
            Cookie(r"^ARRAffinity$"),
        ],
    ),
    tech(
        "github_pages",
        "GitHub Pages",
        HostingCdn,
        &[
            Header("server", r"^GitHub\.com"),
            Header("x-github-request-id", ""),
        ],
    ),
    tech(
        "heroku",
        "Heroku",
        HostingCdn,
        &[Header("via", r"vegur"), Header("server", r"^heroku")],
    ),
    tech(
        "sucuri",
        "Sucuri",
        HostingCdn,
        &[Header("x-sucuri-id", ""), Header("server", r"^Sucuri")],
    ),
    tech(
        "imperva",
        "Imperva",
        HostingCdn,
        &[Header("x-iinfo", ""), Cookie(r"^incap_ses_|^visid_incap_")],
    ),
    tech(
        "bunnycdn",
        "Bunny CDN",
        HostingCdn,
        &[Header("server", r"^BunnyCDN"), Header("cdn-pullzone", "")],
    ),
    tech(
        "wp_engine",
        "WP Engine",
        HostingCdn,
        &[
            Header("x-powered-by", r"WP Engine"),
            Header("wpe-backend", ""),
        ],
    ),
    tech(
        "kinsta",
        "Kinsta",
        HostingCdn,
        &[Header("x-kinsta-cache", "")],
    ),
    tech(
        "pantheon",
        "Pantheon",
        HostingCdn,
        &[Header("x-pantheon-styx-hostname", "")],
    ),
    tech(
        "google_analytics",
        "Google Analytics",
        Analytics,
        &[
            Asset(r"google-analytics\.com/(?:analytics|ga)\.js|googletagmanager\.com/gtag/js"),
            Html(r"gtag\(\s*['\x22]config['\x22]|ga\(\s*['\x22]create"),
        ],
    ),
    tech(
        "google_tag_manager",
        "Google Tag Manager",
        Analytics,
        &[
            Asset(r"googletagmanager\.com/gtm\.js"),
            Html(r"googletagmanager\.com/(?:gtm\.js|ns\.html)\?id=GTM-"),
        ],
    ),
    tech(
        "matomo",
        "Matomo",
        Analytics,
        &[
            Asset(r"matomo\.js|piwik\.js|matomo\.cloud"),
            Html(r"_paq\.push"),
            Cookie(r"^_pk_id\."),
        ],
    ),
    tech(
        "plausible",
        "Plausible",
        Analytics,
        &[Asset(r"plausible\.io/js/")],
    ),
    tech(
        "fathom",
        "Fathom",
        Analytics,
        &[Asset(r"cdn\.usefathom\.com")],
    ),
    tech(
        "hotjar",
        "Hotjar",
        Analytics,
        &[
            Asset(r"static\.hotjar\.com"),
            Html(r"_hjSettings|hotjar\.com/c/hotjar-"),
        ],
    ),
    tech(
        "clarity",
        "Microsoft Clarity",
        Analytics,
        &[Asset(r"clarity\.ms/tag"), Html(r"clarity\.ms/tag/")],
    ),
    tech(
        "contentsquare",
        "Contentsquare",
        Analytics,
        &[Asset(r"t\.contentsquare\.net")],
    ),
    tech(
        "piano_analytics",
        "Piano Analytics (AT Internet)",
        Analytics,
        &[Asset(r"tag\.aticdn\.net|xiti\.com|cdn\.pa-cd\.com")],
    ),
    tech(
        "segment",
        "Segment",
        Analytics,
        &[
            Asset(r"cdn\.segment\.com"),
            Html(r"cdn\.segment\.com/analytics\.js"),
        ],
    ),
    tech(
        "mixpanel",
        "Mixpanel",
        Analytics,
        &[
            Asset(r"cdn\.mxpnl\.com|mixpanel\.com/libs"),
            Html(r"mixpanel\.init\("),
        ],
    ),
    tech(
        "amplitude",
        "Amplitude",
        Analytics,
        &[Asset(r"cdn\.amplitude\.com")],
    ),
    tech(
        "heap",
        "Heap",
        Analytics,
        &[Asset(r"cdn\.heapanalytics\.com"), Html(r"heap\.load\(")],
    ),
    tech(
        "intercom",
        "Intercom",
        Chat,
        &[
            Asset(r"widget\.intercom\.io|js\.intercomcdn\.com"),
            Html(r"widget\.intercom\.io|window\.intercomSettings"),
            Cookie(r"^intercom-(?:id|session)-"),
        ],
    ),
    tech(
        "crisp",
        "Crisp",
        Chat,
        &[
            Asset(r"client\.crisp\.chat"),
            Html(r"CRISP_WEBSITE_ID|client\.crisp\.chat"),
        ],
    ),
    tech(
        "zendesk",
        "Zendesk",
        Chat,
        &[
            Asset(r"static\.zdassets\.com|\.zendesk\.com/embeddable"),
            Html(r"static\.zdassets\.com"),
        ],
    ),
    tech(
        "drift",
        "Drift",
        Chat,
        &[Asset(r"js\.driftt\.com"), Html(r"js\.driftt\.com")],
    ),
    tech(
        "tawk",
        "Tawk.to",
        Chat,
        &[Asset(r"embed\.tawk\.to"), Html(r"embed\.tawk\.to")],
    ),
    tech(
        "livechat",
        "LiveChat",
        Chat,
        &[
            Asset(r"cdn\.livechatinc\.com"),
            Html(r"cdn\.livechatinc\.com"),
        ],
    ),
    tech(
        "freshworks",
        "Freshdesk / Freshchat",
        Chat,
        &[Asset(r"wchat\.freshchat\.com|widget\.freshworks\.com")],
    ),
    tech("tidio", "Tidio", Chat, &[Asset(r"code\.tidio\.co")]),
    tech(
        "iadvize",
        "iAdvize",
        Chat,
        &[Asset(r"iadvize\.com"), Html(r"iadvize\.com/iadvize\.js")],
    ),
    tech(
        "zoho_salesiq",
        "Zoho SalesIQ",
        Chat,
        &[
            Asset(r"salesiq\.zoho\.(?:com|eu)"),
            Html(r"salesiq\.zoho\.(?:com|eu)"),
        ],
    ),
    tech(
        "hubspot",
        "HubSpot",
        CrmMarketing,
        &[
            Asset(r"js\.hs-scripts\.com|js\.hsforms\.net|js\.hs-analytics\.net|js\.hubspot\.com"),
            Html(r"js\.hs-scripts\.com|hbspt\.forms\.create"),
            Cookie(r"^hubspotutk$|^__hstc$"),
        ],
    ),
    tech(
        "pardot",
        "Salesforce Pardot",
        CrmMarketing,
        &[
            Asset(r"pi\.pardot\.com|go\.pardot\.com"),
            Html(r"piAId\s*=|pi\.pardot\.com"),
        ],
    ),
    tech(
        "salesforce",
        "Salesforce",
        CrmMarketing,
        &[Asset(
            r"\.force\.com/|\.salesforce\.com/|service\.force\.com/embeddedservice",
        )],
    ),
    tech(
        "marketo",
        "Marketo",
        CrmMarketing,
        &[
            Asset(r"munchkin\.marketo\.net|\.marketo\.com/js/forms2"),
            Html(r"Munchkin\.init|mktoForm_"),
            Cookie(r"^_mkto_trk$"),
        ],
    ),
    tech(
        "mailchimp",
        "Mailchimp",
        CrmMarketing,
        &[
            Asset(r"chimpstatic\.com|list-manage\.com"),
            Html(r"list-manage\.com/subscribe"),
        ],
    ),
    tech(
        "brevo",
        "Brevo (Sendinblue)",
        CrmMarketing,
        &[
            Asset(r"sibforms\.com|sendinblue\.com|brevo\.com/js"),
            Html(r"sibforms\.com|sendinblue"),
        ],
    ),
    tech(
        "plezi",
        "Plezi",
        CrmMarketing,
        &[Asset(r"plezi\.co"), Html(r"app\.plezi\.co")],
    ),
    tech(
        "activecampaign",
        "ActiveCampaign",
        CrmMarketing,
        &[Asset(r"\.activehosted\.com|trackcmp\.net")],
    ),
    tech(
        "klaviyo",
        "Klaviyo",
        CrmMarketing,
        &[Asset(r"static\.klaviyo\.com")],
    ),
    tech(
        "meta_pixel",
        "Meta Pixel",
        CrmMarketing,
        &[
            Asset(r"connect\.facebook\.net/[^\s]*/fbevents\.js"),
            Html(r"fbq\(\s*['\x22]init"),
        ],
    ),
    tech(
        "linkedin_insight",
        "LinkedIn Insight Tag",
        CrmMarketing,
        &[
            Asset(r"snap\.licdn\.com/li\.lms-analytics"),
            Html(r"_linkedin_partner_id"),
        ],
    ),
    tech(
        "google_ads",
        "Google Ads",
        CrmMarketing,
        &[Asset(
            r"googleadservices\.com|googlesyndication\.com|googleads\.g\.doubleclick\.net",
        )],
    ),
    tech(
        "shopify",
        "Shopify",
        Ecommerce,
        &[
            Header("x-shopid", ""),
            Header("x-shopify-stage", ""),
            Asset(r"cdn\.shopify\.com"),
            Cookie(r"^_shopify_y$|^_shopify_s$"),
        ],
    ),
    tech(
        "woocommerce",
        "WooCommerce",
        Ecommerce,
        &[
            Asset(r"/plugins/woocommerce/"),
            Html(r"woocommerce-(?:page|cart|checkout)|wc-block-"),
        ],
    ),
    tech(
        "prestashop",
        "PrestaShop",
        Ecommerce,
        &[
            Meta("generator", r"^PrestaShop"),
            Html(r"var prestashop\s*="),
            Cookie(r"^PrestaShop-"),
        ],
    ),
    tech(
        "magento",
        "Magento (Adobe Commerce)",
        Ecommerce,
        &[
            Asset(r"/static/version\d+/frontend/|/skin/frontend/"),
            Html(r"Magento_[A-Z]\w+/|mage/cookies"),
            Cookie(r"^X-Magento-Vary$"),
        ],
    ),
    tech(
        "bigcommerce",
        "BigCommerce",
        Ecommerce,
        &[Asset(r"cdn\d*\.bigcommerce\.com")],
    ),
    tech(
        "salesforce_commerce",
        "Salesforce Commerce Cloud",
        Ecommerce,
        &[Asset(r"demandware\.(?:net|static)|/on/demandware\.")],
    ),
    tech(
        "shopware",
        "Shopware",
        Ecommerce,
        &[Meta("generator", r"^Shopware"), Cookie(r"^sw-states$")],
    ),
    tech(
        "keycloak",
        "Keycloak",
        Auth,
        &[
            Asset(r"/js/keycloak\.js|keycloak-js|/realms/[^/\s]+/protocol/openid-connect"),
            Html(r"/realms/[^/\s'\x22]+/protocol/openid-connect"),
        ],
    ),
    tech(
        "auth0",
        "Auth0",
        Auth,
        &[
            Asset(r"cdn\.auth0\.com|auth0-spa-js|auth0\.min\.js"),
            Html(r"[a-z0-9-]+\.(?:eu\.|us\.)?auth0\.com"),
        ],
    ),
    tech(
        "okta",
        "Okta",
        Auth,
        &[
            Asset(r"oktacdn\.com|okta-signin-widget|okta-auth-js"),
            Html(r"[a-z0-9-]+\.okta(?:preview)?\.com"),
        ],
    ),
    tech(
        "entra_id",
        "Microsoft Entra ID",
        Auth,
        &[
            Html(r"login\.microsoftonline\.com"),
            Asset(r"msal-browser|alcdn\.msauth\.net"),
        ],
    ),
    tech(
        "aws_cognito",
        "Amazon Cognito",
        Auth,
        &[Html(r"\.amazoncognito\.com|cognito-idp\.")],
    ),
    tech(
        "clerk",
        "Clerk",
        Auth,
        &[
            Asset(r"clerk\.(?:accounts\.dev|browser\.js)|@clerk/"),
            Cookie(r"^__clerk_db_jwt$"),
        ],
    ),
    tech(
        "firebase_auth",
        "Firebase Authentication",
        Auth,
        &[
            Asset(r"firebase-auth(?:-compat)?\.js"),
            Html(r"\.firebaseapp\.com/__/auth"),
        ],
    ),
    tech(
        "google_identity",
        "Google Identity Services",
        Auth,
        &[Asset(r"accounts\.google\.com/gsi/client")],
    ),
    tech(
        "stripe",
        "Stripe",
        Payment,
        &[
            Asset(r"js\.stripe\.com"),
            Html(r"js\.stripe\.com/v\d|Stripe\(\s*['\x22]pk_"),
        ],
    ),
    tech(
        "paypal",
        "PayPal",
        Payment,
        &[Asset(r"paypal\.com/sdk/js|paypalobjects\.com")],
    ),
    tech(
        "adyen",
        "Adyen",
        Payment,
        &[Asset(
            r"checkoutshopper-(?:live|test)\.adyen\.com|adyen\.com/checkoutshopper",
        )],
    ),
    tech("mollie", "Mollie", Payment, &[Asset(r"js\.mollie\.com")]),
    tech(
        "braintree",
        "Braintree",
        Payment,
        &[Asset(r"js\.braintreegateway\.com")],
    ),
    tech("payplug", "PayPlug", Payment, &[Asset(r"payplug\.com")]),
    tech("paddle", "Paddle", Payment, &[Asset(r"cdn\.paddle\.com")]),
    tech(
        "chargebee",
        "Chargebee",
        Payment,
        &[Asset(r"js\.chargebee\.com")],
    ),
    tech(
        "klarna",
        "Klarna",
        Payment,
        &[Asset(r"klarna\.com|klarnacdn\.net")],
    ),
    tech(
        "axeptio",
        "Axeptio",
        Other,
        &[Asset(r"static\.axept\.io"), Html(r"axeptioSettings")],
    ),
    tech(
        "didomi",
        "Didomi",
        Other,
        &[
            Asset(r"sdk\.privacy-center\.org"),
            Html(r"didomiConfig|window\.didomiOnReady"),
        ],
    ),
    tech(
        "onetrust",
        "OneTrust",
        Other,
        &[
            Asset(r"cdn\.cookielaw\.org|optanon"),
            Cookie(r"^OptanonConsent$"),
        ],
    ),
    tech(
        "cookiebot",
        "Cookiebot",
        Other,
        &[Asset(r"consent\.cookiebot\.com")],
    ),
    tech(
        "tarteaucitron",
        "tarteaucitron.js",
        Other,
        &[Asset(r"tarteaucitron"), Html(r"tarteaucitron\.init")],
    ),
    tech(
        "recaptcha",
        "Google reCAPTCHA",
        Other,
        &[Asset(
            r"google\.com/recaptcha|gstatic\.com/recaptcha|recaptcha\.net",
        )],
    ),
    tech(
        "hcaptcha",
        "hCaptcha",
        Other,
        &[Asset(r"hcaptcha\.com/1/api\.js|js\.hcaptcha\.com")],
    ),
    tech(
        "sentry",
        "Sentry",
        Other,
        &[
            Asset(r"browser\.sentry-cdn\.com|js\.sentry-cdn\.com"),
            Html(r"[a-f0-9]{32}@[a-z0-9.]*ingest\.sentry\.io"),
        ],
    ),
    tech(
        "datadog_rum",
        "Datadog RUM",
        Other,
        &[
            Asset(r"datadoghq-browser-agent\.com"),
            Html(r"DD_RUM\.init"),
        ],
    ),
    tech(
        "new_relic",
        "New Relic",
        Other,
        &[Asset(r"js-agent\.newrelic\.com"), Html(r"NREUM")],
    ),
    tech(
        "algolia",
        "Algolia",
        Other,
        &[Asset(
            r"algolia(?:search|net)?\b[^\s]*\.js|\.algolia\.net|\.algolianet\.com",
        )],
    ),
    tech(
        "typeform",
        "Typeform",
        Other,
        &[Asset(r"embed\.typeform\.com")],
    ),
    tech(
        "calendly",
        "Calendly",
        Other,
        &[
            Asset(r"assets\.calendly\.com"),
            Html(r"calendly\.com/[a-z0-9_-]+"),
        ],
    ),
    tech(
        "nginx",
        "Nginx",
        Other,
        &[Header("server", r"^nginx(?:/([\d.]+))?")],
    ),
    tech(
        "openresty",
        "OpenResty",
        Other,
        &[Header("server", r"^openresty(?:/([\d.]+))?")],
    ),
    tech(
        "apache",
        "Apache HTTP Server",
        Other,
        &[Header("server", r"^Apache(?:/([\d.]+))?")],
    ),
    tech(
        "iis",
        "Microsoft IIS",
        Other,
        &[Header("server", r"^Microsoft-IIS(?:/([\d.]+))?")],
    ),
    tech(
        "litespeed",
        "LiteSpeed",
        Other,
        &[Header("server", r"^LiteSpeed")],
    ),
    tech("caddy", "Caddy", Other, &[Header("server", r"^Caddy")]),
    tech(
        "varnish",
        "Varnish",
        Other,
        &[Header("x-varnish", ""), Header("via", r"varnish")],
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    Header(&'static str),
    Meta(&'static str),
    Asset,
    Cookie,
    Html,
}

impl Evidence {
    pub fn describe(self) -> String {
        match self {
            Self::Header(name) => format!("en-tête {name}"),
            Self::Meta(name) => format!("balise meta {name}"),
            Self::Asset => "script ou feuille de style".to_owned(),
            Self::Cookie => "cookie".to_owned(),
            Self::Html => "code de la page".to_owned(),
        }
    }
}

struct Compiled {
    matcher: Matcher,
    regex: Regex,
}

fn compile(pattern: &str) -> Regex {
    match RegexBuilder::new(pattern).case_insensitive(true).build() {
        Ok(regex) => regex,
        Err(error) => unreachable!("static pattern {pattern} is invalid: {error}"),
    }
}

fn pattern_of(matcher: Matcher) -> &'static str {
    match matcher {
        Header(_, pattern) | Meta(_, pattern) => pattern,
        Asset(pattern) | Cookie(pattern) | Html(pattern) => pattern,
    }
}

static COMPILED: LazyLock<Vec<Vec<Compiled>>> = LazyLock::new(|| {
    TECH_SIGNATURES
        .iter()
        .map(|signature| {
            signature
                .matchers
                .iter()
                .map(|matcher| Compiled {
                    matcher: *matcher,
                    regex: compile(pattern_of(*matcher)),
                })
                .collect()
        })
        .collect()
});

static META_TAG: LazyLock<Regex> = LazyLock::new(|| compile(r"<meta\s[^>]*>"));
static ASSET_TAG: LazyLock<Regex> = LazyLock::new(|| compile(r"<(?:script|link|iframe)\s[^>]*>"));
static ATTRIBUTE: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"([a-z][a-z0-9:_-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#));
static VERSION: LazyLock<Regex> = LazyLock::new(|| compile(r"^\d+(?:\.\d+)*$"));

fn attributes(tag: &str) -> Vec<(String, &str)> {
    ATTRIBUTE
        .captures_iter(tag)
        .filter_map(|captures| {
            let name = captures.get(1)?.as_str().to_ascii_lowercase();
            let value = captures
                .get(2)
                .or_else(|| captures.get(3))
                .or_else(|| captures.get(4))?
                .as_str();
            Some((name, value))
        })
        .collect()
}

fn bounded(html: &str) -> &str {
    if html.len() <= MAX_SCANNED_HTML {
        return html;
    }
    let mut end = MAX_SCANNED_HTML;
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    &html[..end]
}

pub struct PageEvidence<'a> {
    pub url: &'a str,
    pub headers: &'a [(String, String)],
    pub html: &'a str,
}

struct Parsed<'a> {
    metas: Vec<(String, &'a str)>,
    assets: Vec<&'a str>,
    cookies: Vec<&'a str>,
    html: &'a str,
}

fn parse<'a>(page: &PageEvidence<'a>) -> Parsed<'a> {
    let html = bounded(page.html);
    let mut metas = Vec::new();
    for tag in META_TAG.find_iter(html) {
        let attrs = attributes(tag.as_str());
        let name = attrs
            .iter()
            .find(|(key, _)| key == "name" || key == "property")
            .map(|(_, value)| value.to_ascii_lowercase());
        let content = attrs
            .iter()
            .find(|(key, _)| key == "content")
            .map(|(_, value)| value.trim());
        if let (Some(name), Some(content)) = (name, content) {
            metas.push((name, content));
        }
    }
    let assets = ASSET_TAG
        .find_iter(html)
        .flat_map(|tag| {
            attributes(tag.as_str())
                .into_iter()
                .filter(|(key, _)| key == "src" || key == "href")
                .map(|(_, value)| value)
        })
        .collect();
    let cookies = page
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, value)| value.split('=').next().map(str::trim))
        .filter(|name| !name.is_empty())
        .collect();
    Parsed {
        metas,
        assets,
        cookies,
        html,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub signature: &'static TechSignature,
    pub version: Option<String>,
    pub evidence: Evidence,
    pub source: String,
}

impl Detection {
    pub fn display(&self) -> String {
        match &self.version {
            Some(version) => format!("{} {version}", self.signature.name),
            None => self.signature.name.to_owned(),
        }
    }
}

fn version_of(regex: &Regex, text: &str) -> Option<Option<String>> {
    let captures = regex.captures(text)?;
    Some(
        captures
            .get(1)
            .map(|found| found.as_str().trim_matches('.').to_owned())
            .filter(|version| VERSION.is_match(version)),
    )
}

fn match_one(
    compiled: &Compiled,
    page: &PageEvidence<'_>,
    parsed: &Parsed<'_>,
) -> Option<(Option<String>, Evidence)> {
    match compiled.matcher {
        Header(name, _) => page
            .headers
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
            .find_map(|(_, value)| version_of(&compiled.regex, value.trim()))
            .map(|version| (version, Evidence::Header(name))),
        Meta(name, _) => parsed
            .metas
            .iter()
            .filter(|(key, _)| key == name)
            .find_map(|(_, content)| version_of(&compiled.regex, content))
            .map(|version| (version, Evidence::Meta(name))),
        Asset(_) => parsed
            .assets
            .iter()
            .find_map(|asset| version_of(&compiled.regex, asset))
            .map(|version| (version, Evidence::Asset)),
        Cookie(_) => parsed
            .cookies
            .iter()
            .find_map(|cookie| version_of(&compiled.regex, cookie))
            .map(|version| (version, Evidence::Cookie)),
        Html(_) => {
            version_of(&compiled.regex, parsed.html).map(|version| (version, Evidence::Html))
        }
    }
}

pub fn explains(evidence: Evidence, value: &str) -> bool {
    COMPILED.iter().flatten().any(|compiled| {
        let named = match (compiled.matcher, evidence) {
            (Header(name, pattern), Evidence::Header(wanted))
            | (Meta(name, pattern), Evidence::Meta(wanted)) => {
                name == wanted && !pattern.is_empty()
            }
            _ => false,
        };
        named && compiled.regex.is_match(value.trim())
    })
}

pub fn detect(pages: &[PageEvidence<'_>]) -> Vec<Detection> {
    let mut found: Vec<Detection> = Vec::new();
    for page in pages {
        let parsed = parse(page);
        for (signature, compiled) in TECH_SIGNATURES.iter().zip(COMPILED.iter()) {
            for matcher in compiled {
                let Some((version, evidence)) = match_one(matcher, page, &parsed) else {
                    continue;
                };
                match found
                    .iter_mut()
                    .find(|known| known.signature.id == signature.id)
                {
                    Some(known) => {
                        if known.version.is_none() && version.is_some() {
                            known.version = version;
                            known.evidence = evidence;
                            page.url.clone_into(&mut known.source);
                        }
                    }
                    None => found.push(Detection {
                        signature,
                        version,
                        evidence,
                        source: page.url.to_owned(),
                    }),
                }
            }
        }
    }
    found.sort_by_key(|detection| detection.signature.category);
    found
}

#[cfg(test)]
mod tests;
