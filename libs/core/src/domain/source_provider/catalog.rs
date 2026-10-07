use super::descriptor::{
    AuthSpec, Capabilities, FieldSpec, FilterSupport, ProviderCategory, SourceProviderDescriptor,
};

pub const BOAMP: &str = "boamp";
pub const FRANCE_TRAVAIL: &str = "france_travail";

const BOAMP_DEFAULT_URL: &str =
    "https://boamp-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/boamp/records";
const FRANCE_TRAVAIL_TOKEN_URL: &str =
    "https://entreprise.francetravail.fr/connexion/oauth2/access_token?realm=%2Fpartenaire";
const FRANCE_TRAVAIL_API_URL: &str = "https://api.francetravail.io/partenaire/offresdemploi/v2";

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

static CATALOG: [SourceProviderDescriptor; 2] = [
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
];

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
    fn lookup_by_key() {
        assert_eq!(descriptor("boamp").map(|d| d.name), Some("BOAMP"));
        assert!(descriptor("missing").is_none());
    }
}
