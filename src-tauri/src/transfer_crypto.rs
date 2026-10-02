use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Marker written into encrypted connection files. Detection on import is driven by this
/// value, never by the file extension: files get renamed and moved around freely.
pub const ENCRYPTED_CONNECTION_FORMAT: &str = "datanexa-connections-encrypted";
pub const ENCRYPTED_CONNECTION_VERSION: u16 = 1;

/// Sentinels the frontend maps to localized text. Every other error is a plain message.
pub const PASSWORD_REQUIRED: &str = "password_required";
pub const WRONG_PASSWORD: &str = "wrong_password";

/// Shown whenever a file is not a connection file this build can read, whether that is a
/// foreign format, a future container version, or parameters outside the accepted range.
pub const UNSUPPORTED_FILE: &str = "Unsupported DataNexa connection import file.";

pub const MIN_PASSWORD_LENGTH: usize = 8;

const CIPHER_NAME: &str = "aes-256-gcm";
const KDF_NAME: &str = "argon2id";
/// Bound into the AEAD as associated data so the container identity and version cannot be
/// swapped underneath a ciphertext. A fixed string keeps it byte-identical on both sides;
/// re-serializing the header would be sensitive to key order and whitespace.
const ASSOCIATED_DATA: &[u8] = b"datanexa-connections-encrypted:v1";
const SALT_LENGTH: usize = 16;
const NONCE_LENGTH: usize = 12;
const KEY_LENGTH: usize = 32;
const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 1;

/// Bounds applied to KDF parameters read back from a file. Those parameters are attacker
/// controlled, so without a ceiling a crafted file could make Argon2 allocate gigabytes or
/// spin for hours. The limits sit far above anything this app writes, leaving room to raise
/// the cost later without breaking files that are already in circulation.
const MAX_KDF_MEMORY_KIB: u32 = 1024 * 1024;
const MAX_KDF_ITERATIONS: u32 = 16;
const MAX_KDF_PARALLELISM: u32 = 8;

/// KDF parameters travel with the file so they can be raised later without orphaning files
/// written by earlier versions.
#[derive(Debug, Serialize, Deserialize)]
pub struct KdfDescriptor {
    pub name: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub salt: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CipherDescriptor {
    pub name: String,
    pub nonce: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedConnectionFile {
    pub format: String,
    pub version: u16,
    pub exported_at: String,
    pub kdf: KdfDescriptor,
    pub cipher: CipherDescriptor,
    /// Base64 of ciphertext with the GCM tag appended.
    pub payload: String,
}

pub fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < MIN_PASSWORD_LENGTH {
        return Err(format!(
            "The connection encryption password must be at least {MIN_PASSWORD_LENGTH} characters."
        ));
    }
    Ok(())
}

fn checked_kdf_params(params: &KdfDescriptor) -> Result<(), String> {
    let outside_bounds = params.memory_kib < 8
        || params.memory_kib > MAX_KDF_MEMORY_KIB
        || params.iterations < 1
        || params.iterations > MAX_KDF_ITERATIONS
        || params.parallelism < 1
        || params.parallelism > MAX_KDF_PARALLELISM;
    if outside_bounds {
        return Err(UNSUPPORTED_FILE.to_string());
    }
    Ok(())
}

fn derive_key(
    password: &str,
    salt: &[u8],
    params: &KdfDescriptor,
) -> Result<Zeroizing<[u8; KEY_LENGTH]>, String> {
    let argon2_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LENGTH),
    )
    .map_err(|error| format!("Unsupported key derivation parameters: {error}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);
    let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|error| format!("Unsupported key derivation parameters: {error}"))?;
    Ok(key)
}

pub fn encrypt_connection_file(
    plaintext: &[u8],
    password: &str,
) -> Result<EncryptedConnectionFile, String> {
    validate_password(password)?;

    let mut salt = [0u8; SALT_LENGTH];
    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);

    let kdf = KdfDescriptor {
        name: KDF_NAME.to_string(),
        memory_kib: ARGON2_MEMORY_KIB,
        iterations: ARGON2_ITERATIONS,
        parallelism: ARGON2_PARALLELISM,
        salt: BASE64.encode(salt),
    };
    let key = derive_key(password, &salt, &kdf)?;

    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|_| "Invalid encryption key.".to_string())?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: ASSOCIATED_DATA,
            },
        )
        .map_err(|_| "The connection file could not be encrypted.".to_string())?;

    Ok(EncryptedConnectionFile {
        format: ENCRYPTED_CONNECTION_FORMAT.to_string(),
        version: ENCRYPTED_CONNECTION_VERSION,
        exported_at: Utc::now().to_rfc3339(),
        kdf,
        cipher: CipherDescriptor {
            name: CIPHER_NAME.to_string(),
            nonce: BASE64.encode(nonce_bytes),
        },
        payload: BASE64.encode(ciphertext),
    })
}

pub fn decrypt_connection_file(
    file: &EncryptedConnectionFile,
    password: &str,
) -> Result<Zeroizing<Vec<u8>>, String> {
    if file.format != ENCRYPTED_CONNECTION_FORMAT || file.version != ENCRYPTED_CONNECTION_VERSION {
        return Err(UNSUPPORTED_FILE.to_string());
    }
    if file.kdf.name != KDF_NAME || file.cipher.name != CIPHER_NAME {
        return Err(UNSUPPORTED_FILE.to_string());
    }

    let salt = BASE64
        .decode(&file.kdf.salt)
        .map_err(|_| WRONG_PASSWORD.to_string())?;
    let nonce_bytes = BASE64
        .decode(&file.cipher.nonce)
        .map_err(|_| WRONG_PASSWORD.to_string())?;
    let ciphertext = BASE64
        .decode(&file.payload)
        .map_err(|_| WRONG_PASSWORD.to_string())?;
    if nonce_bytes.len() != NONCE_LENGTH {
        return Err(WRONG_PASSWORD.to_string());
    }
    checked_kdf_params(&file.kdf)?;

    let key = derive_key(password, &salt, &file.kdf)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| WRONG_PASSWORD.to_string())?;
    // A wrong password and a tampered ciphertext are indistinguishable here by design; both
    // surface as an authentication failure.
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: ciphertext.as_ref(),
                aad: ASSOCIATED_DATA,
            },
        )
        .map_err(|_| WRONG_PASSWORD.to_string())?;

    Ok(Zeroizing::new(plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &[u8] = br#"{"format":"datanexa-connections","version":3,"exported_at":"2026-09-21T00:00:00Z","connections":[{"name":"local","type":"mysql","password":"placeholder-not-a-real-secret"}]}"#;
    const PASSWORD: &str = "correct horse battery";

    #[test]
    fn round_trip_recovers_the_exact_payload() {
        let file = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");
        let plaintext = decrypt_connection_file(&file, PASSWORD).expect("decrypt");
        assert_eq!(plaintext.as_slice(), PAYLOAD);
    }

    #[test]
    fn container_survives_a_json_cycle() {
        let file = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");
        let serialized = serde_json::to_string(&file).expect("serialize");
        let parsed: EncryptedConnectionFile = serde_json::from_str(&serialized).expect("parse");
        let plaintext = decrypt_connection_file(&parsed, PASSWORD).expect("decrypt");
        assert_eq!(plaintext.as_slice(), PAYLOAD);
    }

    #[test]
    fn every_export_gets_a_fresh_salt_nonce_and_ciphertext() {
        let first = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");
        let second = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");
        assert_ne!(first.kdf.salt, second.kdf.salt);
        assert_ne!(first.cipher.nonce, second.cipher.nonce);
        assert_ne!(first.payload, second.payload);
    }

    #[test]
    fn wrong_password_is_rejected() {
        let file = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");
        let error = decrypt_connection_file(&file, "wrong password").expect_err("must fail");
        assert_eq!(error, WRONG_PASSWORD);
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let mut file = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");
        let mut bytes = BASE64.decode(&file.payload).expect("base64 decode");
        bytes[0] ^= 0xff;
        file.payload = BASE64.encode(&bytes);
        let error = decrypt_connection_file(&file, PASSWORD).expect_err("must fail");
        assert_eq!(error, WRONG_PASSWORD);
    }

    #[test]
    fn tampered_salt_is_rejected() {
        let mut file = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");
        let mut salt = BASE64.decode(&file.kdf.salt).expect("base64 decode");
        salt[0] ^= 0xff;
        file.kdf.salt = BASE64.encode(&salt);
        let error = decrypt_connection_file(&file, PASSWORD).expect_err("must fail");
        assert_eq!(error, WRONG_PASSWORD);
    }

    #[test]
    fn short_passwords_are_rejected_on_export() {
        assert!(encrypt_connection_file(PAYLOAD, "short").is_err());
        assert!(encrypt_connection_file(PAYLOAD, "").is_err());
    }

    /// Guards the reason the KDF parameters live in the container rather than in this file:
    /// raising the Argon2 cost must not orphan files that earlier builds already wrote.
    #[test]
    fn files_written_with_cheaper_parameters_still_open() {
        let mut salt = [0u8; SALT_LENGTH];
        let mut nonce = [0u8; NONCE_LENGTH];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce);

        let cheaper = KdfDescriptor {
            name: KDF_NAME.to_string(),
            memory_kib: 19 * 1024,
            iterations: 2,
            parallelism: 1,
            salt: BASE64.encode(salt),
        };
        let key = derive_key(PASSWORD, &salt, &cheaper).expect("derive");
        let cipher = Aes256Gcm::new_from_slice(key.as_ref()).expect("cipher");
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: PAYLOAD,
                    aad: ASSOCIATED_DATA,
                },
            )
            .expect("encrypt");

        let older_file = EncryptedConnectionFile {
            format: ENCRYPTED_CONNECTION_FORMAT.to_string(),
            version: ENCRYPTED_CONNECTION_VERSION,
            exported_at: "2026-01-01T00:00:00Z".to_string(),
            kdf: cheaper,
            cipher: CipherDescriptor {
                name: CIPHER_NAME.to_string(),
                nonce: BASE64.encode(nonce),
            },
            payload: BASE64.encode(ciphertext),
        };

        let plaintext = decrypt_connection_file(&older_file, PASSWORD).expect("decrypt");
        assert_eq!(plaintext.as_slice(), PAYLOAD);
    }

    #[test]
    fn unknown_container_versions_are_rejected_before_decryption() {
        let mut file = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");
        file.version = ENCRYPTED_CONNECTION_VERSION + 1;
        let error = decrypt_connection_file(&file, PASSWORD).expect_err("must fail");
        assert_ne!(
            error, WRONG_PASSWORD,
            "a future version must not look like a bad password"
        );
    }

    fn clone_file(file: &EncryptedConnectionFile) -> EncryptedConnectionFile {
        EncryptedConnectionFile {
            format: file.format.clone(),
            version: file.version,
            exported_at: file.exported_at.clone(),
            kdf: KdfDescriptor {
                name: file.kdf.name.clone(),
                memory_kib: file.kdf.memory_kib,
                iterations: file.kdf.iterations,
                parallelism: file.kdf.parallelism,
                salt: file.kdf.salt.clone(),
            },
            cipher: CipherDescriptor {
                name: file.cipher.name.clone(),
                nonce: file.cipher.nonce.clone(),
            },
            payload: file.payload.clone(),
        }
    }

    /// The KDF parameters come from the file, so a crafted one must not be able to make Argon2
    /// allocate gigabytes or spin for hours. Each case has to be refused by the bounds check
    /// rather than by authentication, otherwise Argon2 would already have done the work.
    #[test]
    fn absurd_kdf_parameters_are_refused_before_deriving() {
        let file = encrypt_connection_file(PAYLOAD, PASSWORD).expect("encrypt");

        let mut greedy = clone_file(&file);
        greedy.kdf.memory_kib = MAX_KDF_MEMORY_KIB + 1;
        assert_eq!(
            decrypt_connection_file(&greedy, PASSWORD),
            Err(UNSUPPORTED_FILE.to_string())
        );

        let mut endless = clone_file(&file);
        endless.kdf.iterations = MAX_KDF_ITERATIONS + 1;
        assert_eq!(
            decrypt_connection_file(&endless, PASSWORD),
            Err(UNSUPPORTED_FILE.to_string())
        );

        let mut degenerate = clone_file(&file);
        degenerate.kdf.memory_kib = 0;
        assert_eq!(
            decrypt_connection_file(&degenerate, PASSWORD),
            Err(UNSUPPORTED_FILE.to_string())
        );

        let mut overthreaded = clone_file(&file);
        overthreaded.kdf.parallelism = MAX_KDF_PARALLELISM + 1;
        assert_eq!(
            decrypt_connection_file(&overthreaded, PASSWORD),
            Err(UNSUPPORTED_FILE.to_string())
        );
    }
}
