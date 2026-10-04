use serde::{Deserialize, Serialize};

use crate::domain::models::{claims::Claims, client::Client, user::User};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Identity {
    User(User),
    Client(Client),
}

impl Identity {
    pub fn id(&self) -> &str {
        match self {
            Identity::User(u) => &u.id,
            Identity::Client(c) => &c.id,
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self, Identity::User(_))
    }

    pub fn is_client(&self) -> bool {
        matches!(self, Identity::Client(_))
    }

    pub fn username(&self) -> &str {
        match self {
            Identity::User(u) => &u.username,
            Identity::Client(c) => &c.client_id,
        }
    }

    pub fn roles(&self) -> &[String] {
        match self {
            Identity::User(u) => &u.roles,
            Identity::Client(c) => &c.roles,
        }
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles().iter().any(|r| r == role)
    }
}

/// Realm roles as the identity provider sends them: `realm_access.roles`, a
/// JSON array of strings that lands in [`Claims::extra`].
///
/// These were parsed into `extra` and then dropped on the floor - both identity
/// branches hardcoded an empty vector - so the policy engine's super-admin
/// bypass read an empty list and could never fire. Reading them here is what
/// makes that control exist.
///
/// The claim is inside a token whose signature has already been verified
/// against the realm's key set, so its contents are whatever the provider put
/// there and are not caller-influenceable. That matters: the bypass grants
/// every action on every organization.
fn realm_roles(extra: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    extra
        .get("realm_access")
        .and_then(|access| access.get("roles"))
        .and_then(|roles| roles.as_array())
        .map(|roles| {
            roles
                .iter()
                .filter_map(|role| role.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

impl From<Claims> for Identity {
    fn from(claims: Claims) -> Self {
        let roles = realm_roles(&claims.extra);

        if let Some(client_id) = claims.client_id {
            Identity::Client(Client {
                id: claims.sub.0,
                client_id,
                roles,
                scopes: Vec::new(),
            })
        } else {
            Identity::User(User {
                id: claims.sub.0.clone(),
                email: claims.email,
                name: claims.name,
                roles,
                username: claims.preferred_username.unwrap_or(claims.sub.0),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::domain::models::{
        claims::{Audience, Claims},
        identity::Identity,
    };

    fn create_user_claims() -> Claims {
        Claims {
            sub: crate::domain::models::claims::Subject("user-123".to_string()),
            iss: "https://auth.ferriscord.com".to_string(),
            aud: Some(Audience::Single("ferriscord-api".to_string())),
            email: Some("john.doe@example.com".to_string()),
            email_verified: Some(true),
            exp: None,
            name: Some("John Doe".to_string()),
            preferred_username: Some("johndoe".to_string()),
            given_name: Some("John".to_string()),
            family_name: Some("Doe".to_string()),
            scope: "openid profile email".to_string(),
            client_id: None,
            extra: {
                let mut map = serde_json::Map::new();
                map.insert(
                    "realm_access".to_string(),
                    json!({
                        "roles": ["user", "moderator"]
                    }),
                );
                map
            },
        }
    }

    fn create_service_account_claims() -> Claims {
        Claims {
            sub: crate::domain::models::claims::Subject("service-123".to_string()),
            iss: "https://auth.ferriscord.com".to_string(),
            aud: Some(Audience::Single("ferriscord-api".to_string())),
            email: None,
            email_verified: Some(false),
            name: None,
            exp: None,
            preferred_username: Some("service-account-bot".to_string()),
            given_name: None,
            family_name: None,
            scope: "admin:all read:users write:messages".to_string(),
            client_id: Some("ferriscord-bot".to_string()),
            extra: {
                let mut map = serde_json::Map::new();
                map.insert(
                    "realm_access".to_string(),
                    json!({
                        "roles": ["service", "bot"]
                    }),
                );
                map
            },
        }
    }

    #[test]
    fn test_claims_to_identity_user() {
        let claims = create_user_claims();
        let identity: Identity = claims.into();

        match identity {
            Identity::User(user) => {
                assert_eq!(user.id, "user-123");
                assert_eq!(user.username, "johndoe");
                assert_eq!(user.email, Some("john.doe@example.com".to_string()));
                assert_eq!(user.name, Some("John Doe".to_string()));
            }
            Identity::Client(_) => panic!("Expected User, got Client"),
        }
    }

    #[test]
    fn test_claims_to_identity_service_account() {
        let claims = create_service_account_claims();
        let identity: Identity = claims.into();

        match identity {
            Identity::Client(client) => {
                assert_eq!(client.id, "service-123");
                assert_eq!(client.client_id, "ferriscord-bot");
            }
            Identity::User(_) => panic!("Expected Client, got User"),
        }
    }

    #[test]
    fn test_identity_accessors_for_user() {
        let claims = create_user_claims();
        let identity: Identity = claims.into();

        assert!(identity.is_user());
        assert!(!identity.is_client());
        assert_eq!(identity.id(), "user-123");
        assert_eq!(identity.username(), "johndoe");
        assert_eq!(identity.roles(), ["user", "moderator"]);
        assert!(identity.has_role("moderator"));
        assert!(!identity.has_role("admin"));
    }

    #[test]
    fn test_identity_accessors_for_client() {
        let claims = create_service_account_claims();
        let identity: Identity = claims.into();

        assert!(identity.is_client());
        assert!(!identity.is_user());
        assert_eq!(identity.id(), "service-123");
        assert_eq!(identity.username(), "ferriscord-bot");
        assert_eq!(identity.roles(), ["service", "bot"]);
        assert!(identity.has_role("service"));
    }
}

#[cfg(test)]
mod realm_role_tests {
    use serde_json::json;

    use crate::domain::models::{
        claims::{Audience, Claims, Subject},
        identity::Identity,
    };

    fn claims_with(extra: serde_json::Map<String, serde_json::Value>) -> Claims {
        Claims {
            sub: Subject("user-1".to_owned()),
            iss: "https://issuer.example".to_owned(),
            aud: Some(Audience::Single("oxid".to_owned())),
            exp: None,
            email: None,
            email_verified: None,
            name: None,
            preferred_username: Some("alice".to_owned()),
            given_name: None,
            family_name: None,
            scope: "openid".to_owned(),
            client_id: None,
            extra,
        }
    }

    #[test]
    fn realm_roles_reach_the_identity() {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "realm_access".to_owned(),
            json!({ "roles": ["oxid:admin"] }),
        );

        let identity: Identity = claims_with(extra).into();

        assert!(identity.has_role("oxid:admin"));
    }

    #[test]
    fn a_token_without_realm_access_carries_no_roles() {
        let identity: Identity = claims_with(serde_json::Map::new()).into();
        assert!(identity.roles().is_empty());
    }

    #[test]
    fn a_malformed_realm_access_is_not_fatal() {
        // A provider sending the wrong shape must not grant anything, and must
        // not panic either.
        for shape in [
            json!({ "roles": "oxid:admin" }),
            json!("oxid:admin"),
            json!(null),
        ] {
            let mut extra = serde_json::Map::new();
            extra.insert("realm_access".to_owned(), shape);
            let identity: Identity = claims_with(extra).into();
            assert!(identity.roles().is_empty());
        }
    }

    #[test]
    fn non_string_entries_are_dropped_rather_than_stringified() {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "realm_access".to_owned(),
            json!({ "roles": ["viewer", 42, null, "editor"] }),
        );

        let identity: Identity = claims_with(extra).into();

        assert_eq!(identity.roles(), ["viewer", "editor"]);
    }
}
