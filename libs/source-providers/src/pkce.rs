use aws_lc_rs::{
    digest::{SHA256, digest},
    rand::fill,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

const TOKEN_BYTES: usize = 32;

pub fn random_token() -> Option<String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    fill(&mut bytes).ok()?;
    Some(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_the_rfc_7636_example() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn tokens_are_url_safe_and_distinct() {
        let a = random_token().expect("random");
        let b = random_token().expect("random");
        assert_eq!(a.len(), 43);
        assert_ne!(a, b);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }
}
