use std::io;

use application::account::{PasswordHasher, TokenMint};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher as _, PasswordVerifier as _},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

const TOKEN_BYTES: usize = 32;

#[derive(Default)]
pub struct Argon2Hasher;

impl PasswordHasher for Argon2Hasher {
    fn hash(&self, password: &str) -> Result<String, io::Error> {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|error| io::Error::other(format!("could not hash the password: {error}")))
    }

    fn verify(&self, password: &str, hash: &str) -> bool {
        Argon2::default()
            .verify_password(password.as_bytes(), hash)
            .is_ok()
    }
}

#[derive(Default)]
pub struct RandomTokens;

impl TokenMint for RandomTokens {
    fn mint(&self) -> String {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes).expect("the os random source is available");
        URL_SAFE_NO_PAD.encode(bytes)
    }

    fn digest(&self, token: &str) -> Vec<u8> {
        Sha256::digest(token.as_bytes()).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hashed_password_verifies_and_a_wrong_one_does_not() {
        let hasher = Argon2Hasher;
        let hash = hasher.hash("correct horse battery").unwrap();

        assert!(hash.starts_with("$argon2id$"));
        assert!(hasher.verify("correct horse battery", &hash));
        assert!(!hasher.verify("wrong horse battery", &hash));
        assert!(!hasher.verify("anything", "not a phc string"));
    }

    #[test]
    fn tokens_are_unique_and_digests_are_stable() {
        let tokens = RandomTokens;
        let first = tokens.mint();

        assert_ne!(first, tokens.mint());
        assert_eq!(first.len(), 43);
        assert_eq!(tokens.digest(&first), tokens.digest(&first));
        assert_eq!(tokens.digest(&first).len(), 32);
    }
}
