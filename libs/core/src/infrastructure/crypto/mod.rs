use std::fmt;

use aws_lc_rs::{
    aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey},
    rand::fill,
};
use common::{EncryptionKey, Secret};

use crate::domain::llm::provider::{CipherError, SealedSecret, SecretCipher};

pub struct AesGcmCipher {
    key: LessSafeKey,
}

impl AesGcmCipher {
    pub fn new(key: &EncryptionKey) -> Result<Self, CipherError> {
        let unbound = UnboundKey::new(&AES_256_GCM, key.expose()).map_err(|_| CipherError::Seal)?;
        Ok(Self {
            key: LessSafeKey::new(unbound),
        })
    }
}

impl fmt::Debug for AesGcmCipher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AesGcmCipher([redacted])")
    }
}

impl SecretCipher for AesGcmCipher {
    fn seal(&self, secret: &Secret, context: &[u8]) -> Result<SealedSecret, CipherError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        fill(&mut nonce_bytes).map_err(|_| CipherError::Seal)?;
        let mut ciphertext = secret.expose().as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(context),
                &mut ciphertext,
            )
            .map_err(|_| CipherError::Seal)?;
        Ok(SealedSecret {
            nonce: nonce_bytes.to_vec(),
            ciphertext,
        })
    }

    fn open(&self, sealed: &SealedSecret, context: &[u8]) -> Result<Secret, CipherError> {
        let nonce_bytes: [u8; NONCE_LEN] = sealed
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| CipherError::Open)?;
        let mut buffer = sealed.ciphertext.clone();
        let plaintext = self
            .key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(context),
                &mut buffer,
            )
            .map_err(|_| CipherError::Open)?;
        let value = String::from_utf8(plaintext.to_vec()).map_err(|_| CipherError::Open);
        buffer.fill(0);
        value.map(Secret::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> EncryptionKey {
        let encoded = match byte {
            1 => format!("{}QUE=", "QUFB".repeat(10)),
            _ => format!("{}QkI=", "QkJC".repeat(10)),
        };
        EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()
    }

    fn cipher(byte: u8) -> AesGcmCipher {
        AesGcmCipher::new(&key(byte)).unwrap()
    }

    const PLAIN: &str = "sk-live-0123456789abcdef";

    #[test]
    fn round_trip() {
        let c = cipher(1);
        let sealed = c.seal(&Secret::new(PLAIN), b"ctx").unwrap();
        assert_eq!(c.open(&sealed, b"ctx").unwrap().expose(), PLAIN);
    }

    #[test]
    fn ciphertext_hides_plaintext() {
        let sealed = cipher(1).seal(&Secret::new(PLAIN), b"ctx").unwrap();
        assert!(
            !sealed
                .ciphertext
                .windows(PLAIN.len())
                .any(|w| w == PLAIN.as_bytes())
        );
    }

    #[test]
    fn nonce_differs_per_seal() {
        let c = cipher(1);
        let a = c.seal(&Secret::new(PLAIN), b"ctx").unwrap();
        let b = c.seal(&Secret::new(PLAIN), b"ctx").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[test]
    fn wrong_key_fails() {
        let sealed = cipher(1).seal(&Secret::new(PLAIN), b"ctx").unwrap();
        assert_eq!(
            cipher(2).open(&sealed, b"ctx").unwrap_err(),
            CipherError::Open
        );
    }

    #[test]
    fn wrong_context_fails() {
        let c = cipher(1);
        let sealed = c.seal(&Secret::new(PLAIN), b"provider-a").unwrap();
        assert_eq!(
            c.open(&sealed, b"provider-b").unwrap_err(),
            CipherError::Open
        );
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let c = cipher(1);
        let mut sealed = c.seal(&Secret::new(PLAIN), b"ctx").unwrap();
        sealed.ciphertext[0] ^= 1;
        assert_eq!(c.open(&sealed, b"ctx").unwrap_err(), CipherError::Open);
    }

    #[test]
    fn debug_is_redacted() {
        assert_eq!(format!("{:?}", cipher(1)), "AesGcmCipher([redacted])");
    }
}
