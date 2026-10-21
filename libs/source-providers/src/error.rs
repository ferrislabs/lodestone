use lodestone_core::FetchError;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthFailure {
    #[error("Le service d'authentification est injoignable ({0}).")]
    Unreachable(&'static str),
    #[error("L'authentification a été refusée (HTTP {0}).")]
    Rejected(u16),
    #[error("La réponse d'authentification est invalide.")]
    InvalidResponse,
    #[error("Aucun compte n'est connecté.")]
    NotConnected,
    #[error("Le champ « {0} » n'est pas renseigné.")]
    MissingCredential(&'static str),
    #[error("Les secrets enregistrés ne peuvent pas être déchiffrés.")]
    Cipher,
    #[error("L'adresse du service d'authentification est invalide.")]
    InvalidEndpoint,
}

pub fn fetch_reason(error: &FetchError) -> &'static str {
    match error {
        FetchError::Timeout => "délai dépassé",
        FetchError::NonPublicTarget => "adresse non publique refusée",
        FetchError::RateLimited { .. } => "limite de débit atteinte",
        FetchError::InvalidUrl => "adresse invalide",
        FetchError::Redirect => "redirection refusée",
        FetchError::DisallowedByRobots | FetchError::Network(_) => "connexion impossible",
    }
}
