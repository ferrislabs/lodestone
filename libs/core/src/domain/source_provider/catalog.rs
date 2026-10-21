use super::descriptor::{
    AuthSpec, Capabilities, FieldSpec, FilterSupport, ProviderCategory, SourceProviderDescriptor,
};

pub const BOAMP: &str = "boamp";
pub const FRANCE_TRAVAIL: &str = "france_travail";
pub const DECP: &str = "decp";
pub const TED: &str = "ted";
pub const ATS: &str = "ats";
pub const CERTIFICATE_TRANSPARENCY: &str = "certificate_transparency";
pub const DNS_CNAME: &str = "dns_cname";
pub const HTTP_PROBE: &str = "http_probe";
pub const CONTACTS: &str = "contacts";
pub const COMPANY_RESEARCH: &str = "company_research";

const BOAMP_DEFAULT_URL: &str =
    "https://boamp-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/boamp/records";
const FRANCE_TRAVAIL_TOKEN_URL: &str =
    "https://entreprise.francetravail.fr/connexion/oauth2/access_token?realm=%2Fpartenaire";
const FRANCE_TRAVAIL_API_URL: &str = "https://api.francetravail.io/partenaire/offresdemploi/v2";
const DECP_DEFAULT_URL: &str = "https://www.data.gouv.fr/api/1/datasets/donnees-essentielles-de-la-commande-publique-fichiers-consolides/";
const TED_DEFAULT_URL: &str = "https://api.ted.europa.eu/v3/notices/search";
const CT_DEFAULT_URL: &str = "https://www.gstatic.com/ct/log_list/v3/log_list.json";
const COMPANY_DEFAULT_URL: &str = "https://recherche-entreprises.api.gouv.fr/search";
const URL_HELP: &str = "Utilisée pour tester la connexion ; les collectes lisent la variable d'environnement correspondante.";

const BOAMP_FIELDS: &[FieldSpec] = &[FieldSpec::url("base_url", "URL de l'API", false)
    .with_default(BOAMP_DEFAULT_URL)
    .with_help("Point d'accès OpenDataSoft du jeu de données BOAMP.")];

const FRANCE_TRAVAIL_FIELDS: &[FieldSpec] = &[
    FieldSpec::text("client_id", "Identifiant client", true)
        .with_help("Identifiant de l'application partenaire France Travail."),
    FieldSpec::secret("client_secret", "Clé secrète", true)
        .with_help("Clé secrète de l'application partenaire, jamais affichée."),
    FieldSpec::url("token_url", "URL du jeton", false).with_default(FRANCE_TRAVAIL_TOKEN_URL),
    FieldSpec::url("api_base_url", "URL de l'API", false).with_default(FRANCE_TRAVAIL_API_URL),
];

const DECP_FIELDS: &[FieldSpec] = &[FieldSpec::url("dataset_api_url", "URL de l'API", false)
    .with_default(DECP_DEFAULT_URL)
    .with_help(URL_HELP)];

const TED_FIELDS: &[FieldSpec] = &[FieldSpec::url("base_url", "URL de l'API", false)
    .with_default(TED_DEFAULT_URL)
    .with_help(URL_HELP)];

const CT_FIELDS: &[FieldSpec] =
    &[
        FieldSpec::url("log_list_url", "URL de la liste des journaux", false)
            .with_default(CT_DEFAULT_URL)
            .with_help(URL_HELP),
    ];

const COMPANY_FIELDS: &[FieldSpec] = &[FieldSpec::url("entreprises_url", "URL de l'API", false)
    .with_default(COMPANY_DEFAULT_URL)
    .with_help(URL_HELP)];

const NO_FILTERS: FilterSupport = FilterSupport {
    remote: false,
    countries: false,
    contract: false,
};

const NO_SEARCH: Capabilities = Capabilities {
    keyword_search: false,
    filters: NO_FILTERS,
};

static CATALOG: [SourceProviderDescriptor; 10] = [
    SourceProviderDescriptor {
        key: BOAMP,
        name: "BOAMP",
        description: "Avis de marchés publics publiés au Bulletin officiel des annonces des marchés publics.",
        category: ProviderCategory::PublicProcurement,
        auth: AuthSpec::None,
        fields: BOAMP_FIELDS,
        capabilities: Capabilities {
            keyword_search: true,
            filters: FilterSupport {
                remote: false,
                countries: false,
                contract: false,
            },
        },
        terms_note: Some("Données ouvertes publiées sous Licence Ouverte, sans compte requis."),
    },
    SourceProviderDescriptor {
        key: FRANCE_TRAVAIL,
        name: "France Travail",
        description: "Offres d'emploi de l'API Offres d'emploi v2 de France Travail.",
        category: ProviderCategory::JobsAndMissions,
        auth: AuthSpec::OAuth2ClientCredentials {
            token_url: FRANCE_TRAVAIL_TOKEN_URL,
            scopes: &["api_offresdemploiv2", "o2dsoffre"],
        },
        fields: FRANCE_TRAVAIL_FIELDS,
        capabilities: Capabilities {
            keyword_search: true,
            filters: FilterSupport {
                remote: false,
                countries: false,
                contract: true,
            },
        },
        terms_note: Some(
            "L'accès exige une application partenaire déclarée sur francetravail.io et le respect des conditions d'utilisation de l'API.",
        ),
    },
    SourceProviderDescriptor {
        key: DECP,
        name: "DECP",
        description: "Données essentielles de la commande publique : marchés notifiés et titulaires.",
        category: ProviderCategory::PublicProcurement,
        auth: AuthSpec::None,
        fields: DECP_FIELDS,
        capabilities: Capabilities {
            keyword_search: true,
            filters: NO_FILTERS,
        },
        terms_note: Some(
            "Données ouvertes de data.gouv.fr sous Licence Ouverte, sans compte requis.",
        ),
    },
    SourceProviderDescriptor {
        key: TED,
        name: "TED",
        description: "Avis de marchés publics européens publiés au Tenders Electronic Daily.",
        category: ProviderCategory::PublicProcurement,
        auth: AuthSpec::None,
        fields: TED_FIELDS,
        capabilities: Capabilities {
            keyword_search: true,
            filters: FilterSupport {
                remote: false,
                countries: true,
                contract: false,
            },
        },
        terms_note: Some(
            "API publique de l'Office des publications de l'Union européenne, sans compte requis.",
        ),
    },
    SourceProviderDescriptor {
        key: ATS,
        name: "Pages carrières",
        description: "Offres d'emploi lues sur les pages carrières publiques des entreprises et leurs plateformes de recrutement.",
        category: ProviderCategory::JobsAndMissions,
        auth: AuthSpec::None,
        fields: &[],
        capabilities: Capabilities {
            keyword_search: true,
            filters: NO_FILTERS,
        },
        terms_note: Some(
            "Pages publiques uniquement : robots.txt est respecté et le débit par hôte est limité.",
        ),
    },
    SourceProviderDescriptor {
        key: CERTIFICATE_TRANSPARENCY,
        name: "Certificate Transparency",
        description: "Nouveaux certificats TLS lus dans les journaux publics de transparence des certificats.",
        category: ProviderCategory::TechnicalSignals,
        auth: AuthSpec::None,
        fields: CT_FIELDS,
        capabilities: NO_SEARCH,
        terms_note: Some("Journaux publics en lecture seule, débit par journal limité."),
    },
    SourceProviderDescriptor {
        key: DNS_CNAME,
        name: "DNS CNAME",
        description: "Résolutions DNS des hôtes candidats pour repérer les services tiers utilisés.",
        category: ProviderCategory::TechnicalSignals,
        auth: AuthSpec::None,
        fields: &[],
        capabilities: NO_SEARCH,
        terms_note: Some("Requêtes DNS publiques, sans compte requis."),
    },
    SourceProviderDescriptor {
        key: HTTP_PROBE,
        name: "Sonde HTTP",
        description: "Requêtes HTTP sur les hôtes candidats pour identifier les technologies exposées.",
        category: ProviderCategory::TechnicalSignals,
        auth: AuthSpec::None,
        fields: &[],
        capabilities: NO_SEARCH,
        terms_note: Some(
            "Pages publiques uniquement : robots.txt est respecté et le débit par hôte est limité.",
        ),
    },
    SourceProviderDescriptor {
        key: CONTACTS,
        name: "Contacts publics",
        description: "Contacts trouvés dans security.txt, les pages de contact et les adresses de rôle probables.",
        category: ProviderCategory::Companies,
        auth: AuthSpec::None,
        fields: &[],
        capabilities: NO_SEARCH,
        terms_note: Some(
            "Données publiées par les entreprises elles-mêmes ; robots.txt est respecté. Aucun message n'est envoyé.",
        ),
    },
    SourceProviderDescriptor {
        key: COMPANY_RESEARCH,
        name: "Recherche d'entreprises",
        description: "Fiches entreprises de l'API Recherche d'entreprises (effectif, activité, dirigeants).",
        category: ProviderCategory::Companies,
        auth: AuthSpec::None,
        fields: COMPANY_FIELDS,
        capabilities: NO_SEARCH,
        terms_note: Some(
            "API publique de l'administration française, sans compte requis, sous Licence Ouverte.",
        ),
    },
];

pub fn provider_key_of_source(source: &str) -> Option<&'static str> {
    match source {
        "boamp" => Some(BOAMP),
        "france_travail" => Some(FRANCE_TRAVAIL),
        "decp" => Some(DECP),
        "ted" => Some(TED),
        "ats" => Some(ATS),
        "certificate_transparency" => Some(CERTIFICATE_TRANSPARENCY),
        "dns_cname" => Some(DNS_CNAME),
        "http_probe" => Some(HTTP_PROBE),
        "security_txt" | "public_page" | "role_guess" => Some(CONTACTS),
        "company_research" => Some(COMPANY_RESEARCH),
        _ => None,
    }
}

pub fn catalog() -> &'static [SourceProviderDescriptor] {
    &CATALOG
}

pub fn descriptor(key: &str) -> Option<&'static SourceProviderDescriptor> {
    CATALOG.iter().find(|descriptor| descriptor.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique() {
        for (index, descriptor) in catalog().iter().enumerate() {
            assert!(
                catalog()[index + 1..]
                    .iter()
                    .all(|other| other.key != descriptor.key)
            );
        }
    }

    #[test]
    fn oauth_client_credentials_declare_client_fields() {
        for descriptor in catalog() {
            if descriptor.auth.kind().as_str() == "oauth2_client_credentials" {
                assert!(descriptor.field("client_id").is_some());
                assert!(
                    descriptor
                        .field("client_secret")
                        .is_some_and(|f| f.is_secret())
                );
            }
        }
    }

    #[test]
    fn defaults_are_valid_for_their_kind() {
        for descriptor in catalog() {
            for field in descriptor.fields {
                if let Some(default) = field.default {
                    assert!(!default.is_empty());
                }
            }
        }
    }

    #[test]
    fn every_wired_source_maps_to_a_catalog_provider() {
        let sources = [
            "boamp",
            "france_travail",
            "decp",
            "ted",
            "ats",
            "certificate_transparency",
            "dns_cname",
            "http_probe",
            "security_txt",
            "public_page",
            "role_guess",
            "company_research",
        ];
        for source in sources {
            let key = provider_key_of_source(source).unwrap();
            assert!(descriptor(key).is_some(), "{source}");
        }
        assert!(provider_key_of_source("qualification").is_none());
    }

    #[test]
    fn only_france_travail_needs_credentials() {
        for descriptor in catalog() {
            assert_eq!(
                descriptor.auth.kind().as_str() != "none",
                descriptor.key == FRANCE_TRAVAIL
            );
            assert!(descriptor.terms_note.is_some());
        }
    }

    #[test]
    fn lookup_by_key() {
        assert_eq!(descriptor("boamp").map(|d| d.name), Some("BOAMP"));
        assert!(descriptor("missing").is_none());
    }
}
