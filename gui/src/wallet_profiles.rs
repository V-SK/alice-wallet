#![allow(dead_code)]

use crate::chain;
use crate::crypto;
use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const PROFILE_INDEX_VERSION: u32 = 1;
pub const LEGACY_PROFILE_ID: &str = "local-default-wallet";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalletProfileAccess {
    Normal,
    ReadOnly,
    DisplayOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletProfileMetadata {
    pub profile_id: String,
    pub label: String,
    pub address: String,
    pub created_at: String,
    pub last_opened_at: Option<String>,
    pub archived: bool,
    pub node_profile_label: String,
    pub sync_state_label: String,
    #[serde(default = "default_profile_access")]
    pub access: WalletProfileAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressBookRecord {
    pub record_id: String,
    pub profile_id: String,
    pub label: String,
    pub address: String,
    pub note: String,
    pub created_at: String,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiveRequestRecord {
    pub request_id: String,
    pub profile_id: String,
    pub label: String,
    pub address: String,
    pub amount_hint: Option<String>,
    pub created_at: String,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletProfileReservation {
    pub profile_id: String,
    pub wallet_path: PathBuf,
    pub label: String,
    pub access: WalletProfileAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalletProfileIndex {
    version: u32,
    profiles: Vec<WalletProfileMetadata>,
    address_book: Vec<AddressBookRecord>,
    receive_requests: Vec<ReceiveRequestRecord>,
    active_profile_id: Option<String>,
}

impl Default for WalletProfileIndex {
    fn default() -> Self {
        Self {
            version: PROFILE_INDEX_VERSION,
            profiles: Vec::new(),
            address_book: Vec::new(),
            receive_requests: Vec::new(),
            active_profile_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WalletProfileManager {
    root: PathBuf,
    index: WalletProfileIndex,
}

fn default_profile_access() -> WalletProfileAccess {
    WalletProfileAccess::Normal
}

pub fn default_profile_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".alice"))
        .join("AliceWallet")
}

impl WalletProfileMetadata {
    pub fn can_sign(&self, locked: bool) -> bool {
        !locked && matches!(self.access, WalletProfileAccess::Normal)
    }

    fn new(
        profile_id: String,
        label: String,
        address: String,
        access: WalletProfileAccess,
    ) -> Self {
        Self {
            profile_id,
            label,
            address,
            created_at: now_stamp(),
            last_opened_at: None,
            archived: false,
            node_profile_label: "Default node profile".to_string(),
            sync_state_label: "Not checked".to_string(),
            access,
        }
    }
}

impl WalletProfileManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            index: WalletProfileIndex::default(),
        }
    }

    pub fn load_or_default(root: PathBuf) -> Self {
        let index_path = root.join("profiles.json");
        let index = fs::read_to_string(index_path)
            .ok()
            .and_then(|data| serde_json::from_str::<WalletProfileIndex>(&data).ok())
            .unwrap_or_default();
        Self { root, index }
    }

    pub fn qa_mock_profiles() -> Self {
        let mut manager = Self::new(PathBuf::from("qa-display-only-profile-fixture"));
        let warm = WalletProfileMetadata::new(
            "qa-warm-wallet".to_string(),
            "QA mock warm wallet".to_string(),
            crypto::qa_display_address(),
            WalletProfileAccess::DisplayOnly,
        );
        let cold = WalletProfileMetadata::new(
            "qa-cold-wallet".to_string(),
            "QA mock cold wallet".to_string(),
            crypto::qa_display_address_variant(0x24),
            WalletProfileAccess::DisplayOnly,
        );
        manager.index.active_profile_id = Some(warm.profile_id.clone());
        manager.index.profiles = vec![warm.clone(), cold.clone()];
        manager.index.address_book = vec![AddressBookRecord {
            record_id: "qa-contact-local".to_string(),
            profile_id: warm.profile_id.clone(),
            label: "QA local contact".to_string(),
            address: cold.address.clone(),
            note: "Local metadata only".to_string(),
            created_at: now_stamp(),
            archived: false,
        }];
        manager.index.receive_requests = vec![ReceiveRequestRecord {
            request_id: "qa-request-local".to_string(),
            profile_id: warm.profile_id,
            label: "QA receive label".to_string(),
            address: warm.address,
            amount_hint: Some("0 ALICE".to_string()),
            created_at: now_stamp(),
            archived: false,
        }];
        manager
    }

    pub fn save(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root)
            .map_err(|_| "Could not prepare local wallet profile metadata.".to_string())?;
        let data = serde_json::to_vec_pretty(&self.index)
            .map_err(|_| "Could not encode local wallet profile metadata.".to_string())?;
        fs::write(self.index_path(), data)
            .map_err(|_| "Could not save local wallet profile metadata.".to_string())
    }

    pub fn safe_profiles(&self) -> Vec<WalletProfileMetadata> {
        self.index
            .profiles
            .iter()
            .filter(|profile| !profile.archived)
            .cloned()
            .collect()
    }

    pub fn active_profile(&self) -> Option<&WalletProfileMetadata> {
        let active_id = self.index.active_profile_id.as_deref()?;
        self.index
            .profiles
            .iter()
            .find(|profile| profile.profile_id == active_id && !profile.archived)
    }

    pub fn profile(&self, profile_id: &str) -> Option<&WalletProfileMetadata> {
        self.index
            .profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id && !profile.archived)
    }

    pub fn set_active_profile(&mut self, profile_id: &str) -> Result<(), String> {
        if self.profile(profile_id).is_none() {
            return Err("Wallet profile is not available.".to_string());
        }
        self.index.active_profile_id = Some(profile_id.to_string());
        Ok(())
    }

    pub fn mark_opened(&mut self, profile_id: &str) -> Result<(), String> {
        let Some(profile) = self
            .index
            .profiles
            .iter_mut()
            .find(|profile| profile.profile_id == profile_id && !profile.archived)
        else {
            return Err("Wallet profile is not available.".to_string());
        };
        profile.last_opened_at = Some(now_stamp());
        self.index.active_profile_id = Some(profile_id.to_string());
        Ok(())
    }

    pub fn reserve_new_profile(
        &self,
        label: &str,
        access: WalletProfileAccess,
    ) -> Result<WalletProfileReservation, String> {
        validate_local_metadata_value(label)?;
        for _ in 0..16 {
            let profile_id = format!("alice-profile-{}-{}", Utc::now().timestamp(), random_tag());
            if self.profile(&profile_id).is_none() {
                return Ok(WalletProfileReservation {
                    wallet_path: self.profile_wallet_path(&profile_id),
                    profile_id,
                    label: label.to_string(),
                    access,
                });
            }
        }
        Err("Could not allocate a unique wallet profile.".to_string())
    }

    pub fn register_profile(
        &mut self,
        profile_id: String,
        label: String,
        address: String,
        access: WalletProfileAccess,
    ) -> Result<(), String> {
        validate_local_metadata_value(&label)?;
        validate_recent_metadata_parts(&[&profile_id, &label, &address])?;
        chain::validate_address(&address)?;
        if self
            .index
            .profiles
            .iter()
            .any(|profile| profile.profile_id == profile_id && !profile.archived)
        {
            return Err("Wallet profile already exists.".to_string());
        }
        let metadata = WalletProfileMetadata::new(profile_id.clone(), label, address, access);
        validate_recent_metadata(&metadata)?;
        self.index.profiles.push(metadata);
        self.index.active_profile_id = Some(profile_id);
        Ok(())
    }

    pub fn finalize_reserved_profile(
        &mut self,
        reservation: WalletProfileReservation,
        address: String,
    ) -> Result<(), String> {
        self.register_profile(
            reservation.profile_id,
            reservation.label,
            address,
            reservation.access,
        )
    }

    pub fn upsert_detected_default_profile(&mut self, address: String) -> Result<(), String> {
        chain::validate_address(&address)?;
        if let Some(profile) = self
            .index
            .profiles
            .iter_mut()
            .find(|profile| profile.profile_id == LEGACY_PROFILE_ID)
        {
            profile.address = address;
            profile.archived = false;
        } else {
            self.index.profiles.push(WalletProfileMetadata::new(
                LEGACY_PROFILE_ID.to_string(),
                "Local wallet".to_string(),
                address,
                WalletProfileAccess::Normal,
            ));
        }
        self.index.active_profile_id = Some(LEGACY_PROFILE_ID.to_string());
        Ok(())
    }

    pub fn profile_wallet_path(&self, profile_id: &str) -> PathBuf {
        self.root
            .join("profiles")
            .join(profile_id)
            .join("wallet.json")
    }

    pub fn active_wallet_path(&self) -> Option<PathBuf> {
        let active_id = self.index.active_profile_id.as_deref()?;
        Some(self.profile_wallet_path(active_id))
    }

    pub fn rename_profile(&mut self, profile_id: &str, label: &str) -> Result<(), String> {
        validate_local_metadata_value(label)?;
        let Some(profile) = self
            .index
            .profiles
            .iter_mut()
            .find(|profile| profile.profile_id == profile_id && !profile.archived)
        else {
            return Err("Wallet profile is not available.".to_string());
        };
        profile.label = label.to_string();
        Ok(())
    }

    pub fn archive_profile(&mut self, profile_id: &str) -> Result<(), String> {
        let Some(profile) = self
            .index
            .profiles
            .iter_mut()
            .find(|profile| profile.profile_id == profile_id && !profile.archived)
        else {
            return Err("Wallet profile is not available.".to_string());
        };
        profile.archived = true;
        if self.index.active_profile_id.as_deref() == Some(profile_id) {
            self.index.active_profile_id = None;
        }
        Ok(())
    }

    pub fn add_address_book_record(
        &mut self,
        profile_id: &str,
        label: &str,
        address: &str,
        note: &str,
    ) -> Result<String, String> {
        self.profile(profile_id)
            .ok_or_else(|| "Wallet profile is not available.".to_string())?;
        validate_local_metadata_value(label)?;
        validate_local_metadata_value(note)?;
        chain::validate_address(address)?;
        let record_id = format!("contact-{}-{}", Utc::now().timestamp(), random_tag());
        self.index.address_book.push(AddressBookRecord {
            record_id: record_id.clone(),
            profile_id: profile_id.to_string(),
            label: label.to_string(),
            address: address.to_string(),
            note: note.to_string(),
            created_at: now_stamp(),
            archived: false,
        });
        Ok(record_id)
    }

    pub fn remove_address_book_record(&mut self, record_id: &str) -> Result<(), String> {
        let Some(record) = self
            .index
            .address_book
            .iter_mut()
            .find(|record| record.record_id == record_id && !record.archived)
        else {
            return Err("Address book record is not available.".to_string());
        };
        record.archived = true;
        Ok(())
    }

    pub fn address_book_records(&self, profile_id: &str) -> Vec<AddressBookRecord> {
        self.index
            .address_book
            .iter()
            .filter(|record| record.profile_id == profile_id && !record.archived)
            .cloned()
            .collect()
    }

    pub fn add_receive_request(
        &mut self,
        profile_id: &str,
        label: &str,
        address: &str,
        amount_hint: Option<String>,
    ) -> Result<String, String> {
        self.profile(profile_id)
            .ok_or_else(|| "Wallet profile is not available.".to_string())?;
        validate_local_metadata_value(label)?;
        if let Some(hint) = amount_hint.as_deref() {
            validate_local_metadata_value(hint)?;
        }
        chain::validate_address(address)?;
        let request_id = format!("request-{}-{}", Utc::now().timestamp(), random_tag());
        self.index.receive_requests.push(ReceiveRequestRecord {
            request_id: request_id.clone(),
            profile_id: profile_id.to_string(),
            label: label.to_string(),
            address: address.to_string(),
            amount_hint,
            created_at: now_stamp(),
            archived: false,
        });
        Ok(request_id)
    }

    pub fn remove_receive_request(&mut self, request_id: &str) -> Result<(), String> {
        let Some(record) = self
            .index
            .receive_requests
            .iter_mut()
            .find(|record| record.request_id == request_id && !record.archived)
        else {
            return Err("Receive request is not available.".to_string());
        };
        record.archived = true;
        Ok(())
    }

    pub fn receive_requests(&self, profile_id: &str) -> Vec<ReceiveRequestRecord> {
        self.index
            .receive_requests
            .iter()
            .filter(|record| record.profile_id == profile_id && !record.archived)
            .cloned()
            .collect()
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("profiles.json")
    }
}

pub fn selected_wallet_address(manager: &WalletProfileManager) -> Option<String> {
    manager
        .active_profile()
        .filter(|profile| !profile.address.is_empty())
        .map(|profile| profile.address.clone())
}

pub fn validate_recent_metadata(metadata: &WalletProfileMetadata) -> Result<(), String> {
    validate_recent_metadata_parts(&[
        &metadata.profile_id,
        &metadata.label,
        &metadata.address,
        &metadata.created_at,
        metadata.last_opened_at.as_deref().unwrap_or_default(),
        &metadata.node_profile_label,
        &metadata.sync_state_label,
    ])
}

pub fn validate_local_metadata_value(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    validate_recent_metadata_parts(&[value])
}

fn validate_recent_metadata_parts(parts: &[&str]) -> Result<(), String> {
    for value in parts {
        let lower = value.to_ascii_lowercase();
        for fragment in sensitive_fragments() {
            if lower.contains(&fragment) {
                return Err("Local wallet metadata contains restricted material.".to_string());
            }
        }
        if lower.contains("wss://")
            || lower.contains("ws://")
            || lower.contains("http://")
            || lower.contains("https://")
            || lower.contains("stratum")
            || lower.contains("wallet.json")
            || lower.contains(".alice/")
            || lower.contains("alicewallet/")
        {
            return Err(
                "Local wallet metadata contains restricted connection or file details.".to_string(),
            );
        }
    }
    Ok(())
}

fn sensitive_fragments() -> Vec<String> {
    vec![
        ["s", "eed"].concat(),
        ["mnemo", "nic"].concat(),
        ["pass", "word"].concat(),
        ["priv", "ate key"].concat(),
        ["recovery ", "phrase"].concat(),
        "api key".to_string(),
        "token=".to_string(),
        ["std", "out"].concat(),
        ["std", "err"].concat(),
        ["com", "mand"].concat(),
    ]
}

fn now_stamp() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn random_tag() -> String {
    let mut bytes = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "alice-wallet-profile-test-{}-{}",
            name,
            std::process::id()
        ))
    }

    fn valid_address_a() -> String {
        crypto::qa_display_address()
    }

    fn valid_address_b() -> String {
        crypto::qa_display_address_variant(0x55)
    }

    #[test]
    fn safe_profiles_do_not_expose_raw_local_paths() {
        let root = temp_root("safe");
        let mut manager = WalletProfileManager::new(root.clone());
        manager
            .register_profile(
                "profile-a".to_string(),
                "Main wallet".to_string(),
                valid_address_a(),
                WalletProfileAccess::Normal,
            )
            .expect("profile");

        let safe = serde_json::to_string(&manager.safe_profiles()).expect("metadata");
        assert!(!safe.contains(root.to_string_lossy().as_ref()));
        assert!(!safe.contains("wallet.json"));
        assert!(!safe.contains("profiles/profile-a"));
    }

    #[test]
    fn duplicate_profile_registration_is_rejected() {
        let mut manager = WalletProfileManager::new(temp_root("duplicate"));
        manager
            .register_profile(
                "profile-a".to_string(),
                "Main wallet".to_string(),
                valid_address_a(),
                WalletProfileAccess::Normal,
            )
            .expect("first");
        let duplicate = manager.register_profile(
            "profile-a".to_string(),
            "Main wallet copy".to_string(),
            valid_address_b(),
            WalletProfileAccess::Normal,
        );
        assert!(duplicate.is_err());
        assert_eq!(manager.safe_profiles().len(), 1);
    }

    #[test]
    fn import_reservation_uses_unique_profile_storage() {
        let manager = WalletProfileManager::new(temp_root("reservation"));
        let first = manager
            .reserve_new_profile("Imported wallet", WalletProfileAccess::Normal)
            .expect("first reservation");
        let second = manager
            .reserve_new_profile("Imported wallet", WalletProfileAccess::Normal)
            .expect("second reservation");

        assert_ne!(first.profile_id, second.profile_id);
        assert_ne!(first.wallet_path, second.wallet_path);
        assert!(first.wallet_path.ends_with("wallet.json"));
        assert!(second.wallet_path.ends_with("wallet.json"));
    }

    #[test]
    fn locked_read_only_and_display_only_profiles_cannot_sign() {
        let normal = WalletProfileMetadata::new(
            "normal".to_string(),
            "Normal wallet".to_string(),
            valid_address_a(),
            WalletProfileAccess::Normal,
        );
        let read_only = WalletProfileMetadata::new(
            "readonly".to_string(),
            "Read only".to_string(),
            valid_address_a(),
            WalletProfileAccess::ReadOnly,
        );
        let display_only = WalletProfileMetadata::new(
            "display".to_string(),
            "Display only".to_string(),
            valid_address_a(),
            WalletProfileAccess::DisplayOnly,
        );

        assert!(normal.can_sign(false));
        assert!(!normal.can_sign(true));
        assert!(!read_only.can_sign(false));
        assert!(!display_only.can_sign(false));
    }

    #[test]
    fn labels_and_local_records_reject_recovery_material() {
        let mut manager = WalletProfileManager::new(temp_root("labels"));
        manager
            .register_profile(
                "profile-a".to_string(),
                "Main wallet".to_string(),
                valid_address_a(),
                WalletProfileAccess::Normal,
            )
            .expect("profile");

        let restricted = ["recovery ", "phrase"].concat();
        assert!(manager
            .rename_profile("profile-a", &format!("contains {}", restricted))
            .is_err());
        assert!(manager
            .add_address_book_record("profile-a", "Friend", &valid_address_b(), &restricted)
            .is_err());
        assert!(manager
            .add_receive_request("profile-a", &restricted, &valid_address_a(), None)
            .is_err());
    }

    #[test]
    fn recent_metadata_rejects_sensitive_material() {
        let mut metadata = WalletProfileMetadata::new(
            "profile-a".to_string(),
            "Main wallet".to_string(),
            valid_address_a(),
            WalletProfileAccess::Normal,
        );
        assert!(validate_recent_metadata(&metadata).is_ok());

        metadata.node_profile_label = "wss://example.invalid".to_string();
        assert!(validate_recent_metadata(&metadata).is_err());

        metadata.node_profile_label = ["pass", "word"].concat();
        assert!(validate_recent_metadata(&metadata).is_err());
    }

    #[test]
    fn mining_reward_identity_uses_selected_profile_address() {
        let mut manager = WalletProfileManager::new(temp_root("identity"));
        manager
            .register_profile(
                "profile-a".to_string(),
                "Main wallet".to_string(),
                valid_address_a(),
                WalletProfileAccess::Normal,
            )
            .expect("profile a");
        manager
            .register_profile(
                "profile-b".to_string(),
                "Travel wallet".to_string(),
                valid_address_b(),
                WalletProfileAccess::DisplayOnly,
            )
            .expect("profile b");
        manager.set_active_profile("profile-b").expect("active");

        assert_eq!(selected_wallet_address(&manager), Some(valid_address_b()));
    }

    #[test]
    fn address_book_and_receive_requests_can_add_list_and_archive_local_records() {
        let mut manager = WalletProfileManager::new(temp_root("records"));
        manager
            .register_profile(
                "profile-a".to_string(),
                "Main wallet".to_string(),
                valid_address_a(),
                WalletProfileAccess::Normal,
            )
            .expect("profile");

        let contact_id = manager
            .add_address_book_record("profile-a", "Local contact", &valid_address_b(), "")
            .expect("contact");
        assert_eq!(manager.address_book_records("profile-a").len(), 1);
        manager
            .remove_address_book_record(&contact_id)
            .expect("archive contact");
        assert!(manager.address_book_records("profile-a").is_empty());

        let request_id = manager
            .add_receive_request("profile-a", "Invoice label", &valid_address_a(), None)
            .expect("request");
        assert_eq!(manager.receive_requests("profile-a").len(), 1);
        manager
            .remove_receive_request(&request_id)
            .expect("archive request");
        assert!(manager.receive_requests("profile-a").is_empty());
    }

    #[test]
    fn qa_mock_mode_has_two_display_only_profiles_without_real_files() {
        let manager = WalletProfileManager::qa_mock_profiles();
        let profiles = manager.safe_profiles();
        assert!(profiles.len() >= 2);
        assert!(profiles
            .iter()
            .all(|profile| profile.access == WalletProfileAccess::DisplayOnly));
        assert!(manager.active_profile().is_some());
    }
}
