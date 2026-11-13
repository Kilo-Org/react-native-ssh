//! SSH core shared by the Kotlin and Swift binding layers.
//!
//! Everything reachable from the mobile platforms is exported through uniffi.

uniffi::setup_scaffolding!();

use russh::keys::HashAlg;
use russh::keys::PrivateKey;
use russh::keys::ssh_key::LineEnding;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};

const ED25519_SEED_LEN: usize = 32;

#[derive(Debug, uniffi::Record)]
pub struct GeneratedKey {
    pub private_key_openssh: String,
    pub public_key_openssh: String,
    pub fingerprint_sha256: String,
}

#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum CoreError {
    KeyGeneration { message: String },
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::KeyGeneration { message } => write!(f, "key generation failed: {message}"),
        }
    }
}

impl std::error::Error for CoreError {}

fn key_generation_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::KeyGeneration {
        message: error.to_string(),
    }
}

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[uniffi::export]
pub fn generate_ed25519_key(comment: String) -> Result<GeneratedKey, CoreError> {
    let mut seed = [0u8; ED25519_SEED_LEN];
    getrandom::fill(&mut seed).map_err(key_generation_error)?;

    let pair = Ed25519Keypair::from_seed(&seed);
    let key =
        PrivateKey::new(KeypairData::Ed25519(pair), comment).map_err(key_generation_error)?;

    let private_key_openssh = key
        .to_openssh(LineEnding::LF)
        .map_err(key_generation_error)?
        .to_string();
    let public_key = key.public_key();

    Ok(GeneratedKey {
        private_key_openssh,
        public_key_openssh: public_key.to_openssh().map_err(key_generation_error)?,
        fingerprint_sha256: public_key.fingerprint(HashAlg::Sha256).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_version_is_not_empty() {
        assert_eq!(core_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn generated_key_round_trips() {
        let generated = generate_ed25519_key("scaffold@test".to_string()).expect("key generated");

        assert!(
            generated
                .private_key_openssh
                .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----")
        );
        assert!(generated.public_key_openssh.starts_with("ssh-ed25519 "));
        assert!(generated.fingerprint_sha256.starts_with("SHA256:"));

        let parsed = PrivateKey::from_openssh(&generated.private_key_openssh).expect("re-parsed");
        assert_eq!(
            parsed.public_key().to_openssh().expect("public key"),
            generated.public_key_openssh
        );
        assert_eq!(
            parsed
                .public_key()
                .fingerprint(HashAlg::Sha256)
                .to_string(),
            generated.fingerprint_sha256
        );

        let parsed_public =
            russh::keys::PublicKey::from_openssh(&generated.public_key_openssh).expect("public");
        assert_eq!(parsed_public.comment().as_str(), "scaffold@test");
    }
}
