use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::Aes256Gcm;
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as b64, Engine};
use blake2::{Blake2b512, Digest};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use subxt_signer::sr25519::Keypair as Sr25519Keypair;
use zeroize::Zeroize;

pub const MIN_LEGACY_PBKDF2_ITERATIONS: u32 = 200_000;
pub const ARGON2_ITERATIONS: u32 = 2;
pub const ARGON2_MEMORY_KIB: u32 = 19_456;
pub const ARGON2_PARALLELISM: u32 = 1;
pub const SS58_FORMAT: u16 = 300;
pub const WALLET_VERSION_V2: u32 = 2;
pub const WALLET_VERSION_V3: u32 = 3;
pub const CURRENT_WALLET_VERSION: u32 = WALLET_VERSION_V3;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletPayload {
    pub version: u32,
    pub address: String,
    pub public_key: String,
    pub encrypted_seed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_mnemonic: Option<String>,
    pub salt: String,
    pub nonce_seed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_mnemonic: Option<String>,
    pub kdf: String,
    pub kdf_iterations: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kdf_memory_kib: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kdf_parallelism: Option<u32>,
}

struct SecretSeed {
    bytes: [u8; 32],
}

impl SecretSeed {
    fn from_slice(seed_bytes: &[u8]) -> Result<Self, String> {
        if seed_bytes.len() != 32 {
            return Err("Seed length mismatch".into());
        }

        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(seed_bytes);
        Ok(Self { bytes })
    }

    fn expose(&self) -> &[u8; 32] {
        &self.bytes
    }
}

impl Drop for SecretSeed {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

#[derive(Clone)]
pub struct WalletSecrets {
    pub address: String,
    seed: Arc<SecretSeed>,
}

impl WalletSecrets {
    pub fn to_keypair(&self) -> Result<Sr25519Keypair, String> {
        Sr25519Keypair::from_secret_key(*self.seed.expose()).map_err(|e| e.to_string())
    }
}

pub struct UnlockOutcome {
    pub secrets: WalletSecrets,
    pub upgraded_payload: Option<WalletPayload>,
}

pub fn default_wallet_path() -> PathBuf {
    data_dir().join("AliceWallet").join("wallet.json")
}

pub fn detect_wallet_path() -> PathBuf {
    let primary = default_wallet_path();
    if primary.exists() {
        return primary;
    }

    let legacy = legacy_wallet_path();
    if legacy.exists() {
        return legacy;
    }

    primary
}

/// Rename any existing wallet file to `wallet.json.bak-<timestamp>` so that an
/// import/overwrite can never destroy an old wallet silently.
pub fn backup_existing_wallet(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("wallet.json");
    let backup = path.with_file_name(format!("{}.bak-{}", file_name, ts));
    fs::rename(path, &backup).map_err(|e| format!("Failed to back up existing wallet: {}", e))?;
    Ok(Some(backup))
}

pub fn write_wallet_payload(path: &Path, payload: &WalletPayload) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create wallet directory: {}", e))?;
    }

    let encoded = serde_json::to_vec_pretty(payload)
        .map_err(|e| format!("Failed to serialize wallet payload: {}", e))?;
    let tmp_path = temporary_wallet_path(path);

    let mut file = create_wallet_file(&tmp_path)?;
    file.write_all(&encoded)
        .map_err(|e| format!("Failed to write wallet file: {}", e))?;
    file.flush()
        .map_err(|e| format!("Failed to flush wallet file: {}", e))?;
    file.sync_all()
        .map_err(|e| format!("Failed to sync wallet file: {}", e))?;
    drop(file);

    persist_wallet_file(&tmp_path, path)?;

    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = OpenOptions::new().read(true).open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

pub fn unlock_wallet(payload: &WalletPayload, password: &str) -> Result<UnlockOutcome, String> {
    if !matches!(payload.version, WALLET_VERSION_V2 | WALLET_VERSION_V3) {
        return Err(format!("Unsupported wallet version: {}", payload.version));
    }

    let salt = b64.decode(&payload.salt).map_err(|_| "Invalid salt")?;
    let mut key = derive_wallet_key(payload, password, &salt)?;
    let seed_bytes = decrypt_blob(&payload.encrypted_seed, &payload.nonce_seed, &key)?;

    let seed = SecretSeed::from_slice(&seed_bytes)?;
    let keypair = Sr25519Keypair::from_secret_key(*seed.expose()).map_err(|e| e.to_string())?;
    verify_identity(payload, &keypair)?;

    if let (Some(enc_mnemonic), Some(nonce_mnemonic)) =
        (&payload.encrypted_mnemonic, &payload.nonce_mnemonic)
    {
        let mut decrypted = decrypt_blob(enc_mnemonic, nonce_mnemonic, &key)?;
        let mnemonic =
            std::str::from_utf8(&decrypted).map_err(|_| "Mnemonic is not valid UTF-8")?;
        let mnemonic_pair = keypair_from_phrase(mnemonic)?;
        decrypted.zeroize();
        if mnemonic_pair.public_key().0 != keypair.public_key().0 {
            return Err("Mnemonic does not match wallet seed".into());
        }
    }

    key.zeroize();

    let upgraded_payload = if payload_needs_upgrade(payload) {
        Some(create_wallet_payload_from_seed(seed.expose(), password)?)
    } else {
        None
    };

    Ok(UnlockOutcome {
        secrets: WalletSecrets {
            address: payload.address.clone(),
            seed: Arc::new(seed),
        },
        upgraded_payload,
    })
}

pub fn create_wallet_payload(mnemonic: &str, password: &str) -> Result<WalletPayload, String> {
    let seed_bytes = substrate_seed_from_phrase(mnemonic)?;
    let keypair = Sr25519Keypair::from_secret_key(seed_bytes).map_err(|e| e.to_string())?;
    create_wallet_payload_from_keypair(&keypair, &seed_bytes, password)
}

/// Build a wallet payload from a raw 32-byte sr25519 secret seed (hex,
/// optionally `0x` prefixed). Used for the "Import raw private key" path.
pub fn create_wallet_payload_from_seed_hex(
    seed_hex: &str,
    password: &str,
) -> Result<WalletPayload, String> {
    let trimmed = seed_hex.trim().trim_start_matches("0x").trim_start_matches("0X");
    if trimmed.len() != 64 {
        return Err("Seed must be 0x + 64 hex characters (32 bytes)".into());
    }
    let mut bytes_vec = hex::decode(trimmed)
        .map_err(|_| "Seed contains invalid hex characters".to_string())?;
    if bytes_vec.len() != 32 {
        bytes_vec.zeroize();
        return Err("Decoded seed length is not 32 bytes".into());
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes_vec);
    bytes_vec.zeroize();
    let result = create_wallet_payload_from_seed(&seed, password);
    seed.zeroize();
    result
}

fn create_wallet_payload_from_seed(
    seed_bytes: &[u8; 32],
    password: &str,
) -> Result<WalletPayload, String> {
    let keypair = Sr25519Keypair::from_secret_key(*seed_bytes).map_err(|e| e.to_string())?;
    create_wallet_payload_from_keypair(&keypair, seed_bytes, password)
}

fn create_wallet_payload_from_keypair(
    keypair: &Sr25519Keypair,
    seed_bytes: &[u8],
    password: &str,
) -> Result<WalletPayload, String> {
    let mut salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt);

    let mut key = derive_argon2id_key(
        password,
        &salt,
        ARGON2_ITERATIONS,
        ARGON2_MEMORY_KIB,
        ARGON2_PARALLELISM,
    )?;
    let (encrypted_seed, nonce_seed) = encrypt_blob(seed_bytes, &key)?;
    key.zeroize();

    Ok(WalletPayload {
        version: CURRENT_WALLET_VERSION,
        address: account_id_to_ss58(&keypair.public_key().0, SS58_FORMAT),
        public_key: format!("0x{}", hex::encode(keypair.public_key().0)),
        encrypted_seed,
        encrypted_mnemonic: None,
        salt: b64.encode(salt),
        nonce_seed,
        nonce_mnemonic: None,
        kdf: "argon2id".into(),
        kdf_iterations: ARGON2_ITERATIONS,
        kdf_memory_kib: Some(ARGON2_MEMORY_KIB),
        kdf_parallelism: Some(ARGON2_PARALLELISM),
    })
}

fn derive_wallet_key(
    payload: &WalletPayload,
    password: &str,
    salt: &[u8],
) -> Result<[u8; 32], String> {
    match payload.kdf.as_str() {
        "pbkdf2-sha256" => Ok(derive_pbkdf2_key(
            password,
            salt,
            payload.kdf_iterations.max(MIN_LEGACY_PBKDF2_ITERATIONS),
        )),
        "argon2id" => derive_argon2id_key(
            password,
            salt,
            payload.kdf_iterations.max(1),
            payload.kdf_memory_kib.unwrap_or(ARGON2_MEMORY_KIB),
            payload.kdf_parallelism.unwrap_or(ARGON2_PARALLELISM),
        ),
        other => Err(format!("Unsupported KDF: {}", other)),
    }
}

fn derive_pbkdf2_key(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut key);
    key
}

fn derive_argon2id_key(
    password: &str,
    salt: &[u8],
    iterations: u32,
    memory_kib: u32,
    parallelism: u32,
) -> Result<[u8; 32], String> {
    let params = Params::new(memory_kib, iterations, parallelism, Some(32))
        .map_err(|e| format!("Invalid Argon2 parameters: {}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Argon2 derivation failed: {}", e))?;
    Ok(key)
}

fn encrypt_blob(plaintext: &[u8], key: &[u8; 32]) -> Result<(String, String), String> {
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let cipher = Aes256Gcm::new(key.into());
    let ciphertext = cipher
        .encrypt(&nonce_bytes.into(), plaintext)
        .map_err(|e| format!("encryption failure: {}", e))?;

    Ok((b64.encode(&ciphertext), b64.encode(&nonce_bytes)))
}

fn decrypt_blob(ciphertext_b64: &str, nonce_b64: &str, key: &[u8; 32]) -> Result<Vec<u8>, String> {
    let ciphertext = b64
        .decode(ciphertext_b64)
        .map_err(|_| "Invalid base64 in ciphertext")?;
    let nonce_bytes = b64
        .decode(nonce_b64)
        .map_err(|_| "Invalid base64 in nonce")?;
    if nonce_bytes.len() != 12 {
        return Err("Nonce length mismatch".into());
    }

    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(nonce_bytes.as_slice().into(), ciphertext.as_ref())
        .map_err(|_| "Decryption failed (wrong password or corrupted)".into())
}

fn payload_needs_upgrade(payload: &WalletPayload) -> bool {
    payload.version != CURRENT_WALLET_VERSION
        || payload.kdf != "argon2id"
        || payload.kdf_iterations != ARGON2_ITERATIONS
        || payload.kdf_memory_kib != Some(ARGON2_MEMORY_KIB)
        || payload.kdf_parallelism != Some(ARGON2_PARALLELISM)
        || payload.encrypted_mnemonic.is_some()
        || payload.nonce_mnemonic.is_some()
}

fn verify_identity(payload: &WalletPayload, keypair: &Sr25519Keypair) -> Result<(), String> {
    let address = account_id_to_ss58(&keypair.public_key().0, SS58_FORMAT);
    if address != payload.address {
        return Err("Wallet address mismatch".into());
    }

    let public_key_hex = format!("0x{}", hex::encode(keypair.public_key().0));
    if payload.public_key != public_key_hex {
        return Err("Wallet public key mismatch".into());
    }

    Ok(())
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(legacy_data_dir)
}

fn legacy_wallet_path() -> PathBuf {
    legacy_data_dir().join("wallet.json")
}

fn legacy_data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Cannot find home directory")
        .join(".alice")
}

fn temporary_wallet_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("wallet.json");
    path.with_file_name(format!("{}.tmp-{}", file_name, std::process::id()))
}

fn create_wallet_file(path: &Path) -> Result<std::fs::File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    options
        .open(path)
        .map_err(|e| format!("Failed to open wallet file {}: {}", path.display(), e))
}

fn persist_wallet_file(tmp_path: &Path, final_path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    if final_path.exists() {
        fs::remove_file(final_path)
            .map_err(|e| format!("Failed to replace existing wallet file: {}", e))?;
    }

    fs::rename(tmp_path, final_path).map_err(|e| {
        let _ = fs::remove_file(tmp_path);
        format!(
            "Failed to move wallet file into place ({} -> {}): {}",
            tmp_path.display(),
            final_path.display(),
            e
        )
    })
}

fn keypair_from_phrase(mnemonic: &str) -> Result<Sr25519Keypair, String> {
    let seed_bytes = substrate_seed_from_phrase(mnemonic)?;
    Sr25519Keypair::from_secret_key(seed_bytes).map_err(|e| e.to_string())
}

fn substrate_seed_from_phrase(mnemonic: &str) -> Result<[u8; 32], String> {
    let mnemonic = bip39::Mnemonic::parse(mnemonic).map_err(|e| e.to_string())?;
    let (entropy, len) = mnemonic.to_entropy_array();
    let seed64 = substrate_bip39::seed_from_entropy(&entropy[..len], "")
        .map_err(|e| format!("Failed to derive seed from mnemonic entropy: {:?}", e))?;
    let mut secret = [0u8; 32];
    secret.copy_from_slice(&seed64[..32]);
    Ok(secret)
}

fn account_id_to_ss58(account_id: &[u8; 32], format: u16) -> String {
    let ident = format & 0b0011_1111_1111_1111;
    let mut payload = match ident {
        0..=63 => vec![ident as u8],
        64..=16_383 => {
            let first = ((ident & 0b0000_0000_1111_1100) as u8 >> 2) | 0b0100_0000;
            let second = ((ident >> 8) as u8) | (((ident & 0b0000_0000_0000_0011) as u8) << 6);
            vec![first, second]
        }
        _ => unreachable!("ss58 format is masked to 14 bits"),
    };
    payload.extend(account_id);

    let mut hasher = Blake2b512::new();
    hasher.update(b"SS58PRE");
    hasher.update(&payload);
    let checksum = hasher.finalize();
    payload.extend(&checksum[..2]);

    bs58::encode(payload).into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY_PBKDF2_ITERATIONS: u32 = 600_000;

    #[test]
    fn creates_v3_wallet_without_persisted_mnemonic() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let payload = create_wallet_payload(phrase, "correct horse battery staple").unwrap();

        assert_eq!(payload.version, CURRENT_WALLET_VERSION);
        assert_eq!(payload.kdf, "argon2id");
        assert!(payload.encrypted_mnemonic.is_none());
        assert!(payload.nonce_mnemonic.is_none());
    }

    #[test]
    fn unlocks_legacy_wallet_and_requests_upgrade() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let seed_bytes = substrate_seed_from_phrase(phrase).unwrap();
        let keypair = Sr25519Keypair::from_secret_key(seed_bytes).unwrap();
        let mut salt = [0u8; 32];
        salt.copy_from_slice(&[7u8; 32]);
        let key = derive_pbkdf2_key("password123", &salt, LEGACY_PBKDF2_ITERATIONS);
        let (encrypted_seed, nonce_seed) = encrypt_blob(&seed_bytes, &key).unwrap();
        let (encrypted_mnemonic, nonce_mnemonic) = encrypt_blob(phrase.as_bytes(), &key).unwrap();

        let payload = WalletPayload {
            version: WALLET_VERSION_V2,
            address: account_id_to_ss58(&keypair.public_key().0, SS58_FORMAT),
            public_key: format!("0x{}", hex::encode(keypair.public_key().0)),
            encrypted_seed,
            encrypted_mnemonic: Some(encrypted_mnemonic),
            salt: b64.encode(salt),
            nonce_seed,
            nonce_mnemonic: Some(nonce_mnemonic),
            kdf: "pbkdf2-sha256".into(),
            kdf_iterations: LEGACY_PBKDF2_ITERATIONS,
            kdf_memory_kib: None,
            kdf_parallelism: None,
        };

        let unlocked = unlock_wallet(&payload, "password123").unwrap();
        assert_eq!(unlocked.secrets.address, payload.address);
        assert!(unlocked.upgraded_payload.is_some());
        assert!(unlocked
            .upgraded_payload
            .as_ref()
            .unwrap()
            .encrypted_mnemonic
            .is_none());
    }
}
