use crate::chain::{self, NodeSyncSnapshot, NodeSyncState};
use crate::config::{Lang, Settings};
use crate::crypto::{self, WalletPayload, WalletSecrets};
use crate::history::{self, TxRecord};
use crate::i18n;
use crate::node::{self, NodeMode};
use crate::supervise::node_supervisor::NodeSupervisor;
use crate::supervise::ProcStatus;
use crate::ui;
use crate::wallet_profiles::{
    self, AddressBookRecord, ReceiveRequestRecord, WalletProfileAccess, WalletProfileManager,
    WalletProfileMetadata, WalletProfileReservation, LEGACY_PROFILE_ID,
};
use eframe::egui;
use rand::RngCore;
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use zeroize::Zeroize;

// ────────────────────────────────────────────────────────────────────────────
// Public types used across UI modules
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Receive,
    Send,
    Mining,
    Node,
    History,
    Accounts,
    AddressBook,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    CheckWallet,
    WalletChoice,
    Unlock,
    Create,
    Import,
    Backup,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMethod {
    Mnemonic,
    PrivateKey,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub title: String,
    pub body: String,
    pub ok: bool,
    pub expires_at: Instant,
}

impl Toast {
    pub fn ok(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            ok: true,
            expires_at: Instant::now() + Duration::from_secs(6),
        }
    }
    pub fn err(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            ok: false,
            expires_at: Instant::now() + Duration::from_secs(8),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Async worker plumbing
// ────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum AsyncAction {
    RefreshAll(String, String), // rpc_url, address
    RefreshNodeSync(String),
    Unlock(WalletPayload, String),
    Create(String, String),
    Import(String, String, std::path::PathBuf),
    ImportSeedHex(String, String, std::path::PathBuf),
    /// Start the embedded node from a validated launch plan + child env + pid file.
    StartNode(
        node::NodeLaunchPlan,
        Vec<(String, String)>,
        std::path::PathBuf,
    ),
    /// Request a graceful stop of the embedded node.
    StopNode,
    /// Poll the embedded node's process status into the GUI.
    PollNodeProc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFilter {
    All,
    Send,
}

pub enum AsyncResult {
    Balance(u128),
    NodeSync(NodeSyncSnapshot),
    ConnectionOk,
    ConnectionErr(String),
    UnlockOk(WalletSecrets, Option<WalletPayload>),
    UnlockErr(String),
    CreateOk(WalletPayload, WalletSecrets, String),
    ImportOk(WalletPayload, WalletSecrets, Option<std::path::PathBuf>),
    CreateErr(String),
    SyncErr(String),
    /// Latest embedded-node process status snapshot.
    NodeProc(ProcStatus),
    /// A node start/stop request failed before it could be supervised.
    NodeProcErr(String),
}

// ────────────────────────────────────────────────────────────────────────────

pub struct AliceWalletApp {
    pub qa_mock_mode: bool,
    pub network_disabled: bool,
    pub evidence_redact_secrets: bool,

    pub phase: Phase,
    pub page: Page,

    // wallet
    pub default_wallet_path: std::path::PathBuf,
    pub detected_wallet_path: Option<std::path::PathBuf>,
    pub wallet_path: std::path::PathBuf,
    pub payload: Option<WalletPayload>,
    pub secrets: Option<WalletSecrets>,
    pub profile_manager: WalletProfileManager,
    pub pending_profile_reservation: Option<WalletProfileReservation>,

    // settings
    pub settings: Settings,
    pub settings_lock_draft: String,

    // auth inputs
    pub password_input: String,
    pub password_visible: bool,
    pub confirm_password_input: String,
    pub mnemonic_words: Vec<String>,
    pub private_key_input: String,
    pub private_key_export: String,
    pub private_key_export_password: String,
    pub private_key_export_password_visible: bool,
    pub import_method: ImportMethod,
    pub mnemonic_backup: String,
    pub auth_error: String,
    pub unlock_fail_count: u32,
    pub unlock_block_until: Option<Instant>,

    // dashboard data
    pub balance: Option<u128>,
    pub block_height: Option<u64>,
    pub node_sync: NodeSyncSnapshot,
    pub sync_error: Option<String>,
    pub connection_status: ConnectionState,
    pub last_block_poll: Option<Instant>,
    pub last_data_poll: Option<Instant>,

    // history
    pub history: Vec<TxRecord>,
    pub history_filter: HistoryFilter,
    pub show_receive_qr: bool,
    pub send_recipient: String,
    pub send_amount: String,
    pub send_note: String,
    pub send_review_ready: bool,
    pub send_review_error: Option<String>,
    pub lock_warn_shown: bool,

    // ui ephemeral
    pub address_copied_at: Option<Instant>,
    pub mnemonic_copied_at: Option<Instant>,
    pub clipboard_clear_at: Option<Instant>,
    pub backup_quiz_indices: [usize; 3],
    pub backup_quiz_inputs: [String; 3],
    pub toast: Option<Toast>,
    pub busy: bool,
    pub auth_busy: bool,
    pub refresh_pending: usize,
    pub last_interaction: Instant,

    // embedded node
    pub node_supervisor: NodeSupervisor,
    pub node_proc: ProcStatus,
    pub last_node_proc_poll: Option<Instant>,

    // async
    pub tx: Sender<AsyncAction>,
    pub rx: Receiver<AsyncResult>,
}

impl AliceWalletApp {
    pub fn new(rt: Runtime) -> Self {
        let qa_mock_mode = env_flag("ALICE_WALLET_QA_MOCK");
        let network_disabled = env_flag("ALICE_WALLET_NETWORK_DISABLED");
        let evidence_redact_secrets = env_flag("ALICE_WALLET_EVIDENCE_REDACT_SECRETS");
        let phase40t_evidence_mode =
            !qa_mock_mode && std::env::var_os("ALICE_WALLET_PHASE40T_EVIDENCE_PAGE").is_some();
        let default_wallet_path = if qa_mock_mode {
            std::path::PathBuf::from("qa-display-only-no-wallet-file")
        } else {
            crypto::default_wallet_path()
        };
        let settings = if qa_mock_mode {
            Settings::default()
        } else {
            Settings::load()
        };
        let profile_manager = if qa_mock_mode {
            WalletProfileManager::qa_mock_profiles()
        } else {
            WalletProfileManager::load_or_default(wallet_profiles::default_profile_root())
        };
        let (gui_tx, worker_rx) = channel::<AsyncAction>();
        let (worker_tx, gui_rx) = channel::<AsyncResult>();

        let rt = Arc::new(rt);
        let node_supervisor = NodeSupervisor::new();
        spawn_worker(rt.clone(), worker_rx, worker_tx, node_supervisor.clone());

        let mut app = Self {
            qa_mock_mode,
            network_disabled,
            evidence_redact_secrets,
            phase: Phase::CheckWallet,
            page: Page::Dashboard,
            default_wallet_path: default_wallet_path.clone(),
            detected_wallet_path: None,
            wallet_path: default_wallet_path,
            payload: None,
            secrets: None,
            profile_manager,
            pending_profile_reservation: None,
            settings_lock_draft: settings.auto_lock_minutes.to_string(),
            settings,
            password_input: String::new(),
            password_visible: false,
            confirm_password_input: String::new(),
            mnemonic_words: vec![String::new(); 24],
            private_key_input: String::new(),
            private_key_export: String::new(),
            private_key_export_password: String::new(),
            private_key_export_password_visible: false,
            import_method: ImportMethod::Mnemonic,
            mnemonic_backup: String::new(),
            auth_error: String::new(),
            unlock_fail_count: 0,
            unlock_block_until: None,
            balance: None,
            block_height: None,
            node_sync: NodeSyncSnapshot::unavailable("not_checked"),
            sync_error: None,
            connection_status: ConnectionState::Connecting,
            last_block_poll: None,
            last_data_poll: None,
            history: history::load(),
            history_filter: HistoryFilter::All,
            show_receive_qr: false,
            send_recipient: String::new(),
            send_amount: String::new(),
            send_note: String::new(),
            send_review_ready: false,
            send_review_error: None,
            lock_warn_shown: false,
            address_copied_at: None,
            mnemonic_copied_at: None,
            clipboard_clear_at: None,
            backup_quiz_indices: [0, 8, 16],
            backup_quiz_inputs: [String::new(), String::new(), String::new()],
            toast: None,
            busy: false,
            auth_busy: false,
            refresh_pending: 0,
            last_interaction: Instant::now(),
            node_supervisor,
            node_proc: ProcStatus::stopped(),
            last_node_proc_poll: None,
            tx: gui_tx,
            rx: gui_rx,
        };

        if app.qa_mock_mode {
            app.enable_qa_mock_mode();
        } else {
            app.check_wallet();
            if phase40t_evidence_mode {
                app.enable_phase40t_evidence_mode();
            }
        }
        app
    }

    fn enable_qa_mock_mode(&mut self) {
        self.phase = Phase::Main;
        self.page = qa_page_from_env();
        self.payload = None;
        self.detected_wallet_path = None;
        let address = self
            .profile_manager
            .active_profile()
            .map(|profile| profile.address.clone())
            .unwrap_or_else(crypto::qa_display_address);
        self.secrets = Some(crypto::WalletSecrets::display_only(address));
        self.settings.rpc_url = "wss://alice-wallet-qa.invalid".into();
        self.balance = Some(0);
        self.block_height = None;
        self.node_sync = NodeSyncSnapshot::unavailable("qa_mock_local_fixture_only");
        self.connection_status = ConnectionState::Error;
        self.sync_error = Some("Mock QA mode uses local fixture data only.".to_string());
        self.refresh_pending = 0;

        match std::env::var("ALICE_WALLET_QA_PHASE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "create" => {
                self.secrets = None;
                self.phase = Phase::Create;
            }
            "import" | "recovery" => {
                self.secrets = None;
                self.phase = Phase::Import;
            }
            "unlock" | "lock" => {
                self.secrets = None;
                self.phase = Phase::Unlock;
            }
            "backup" => {
                self.phase = Phase::Backup;
                self.mnemonic_backup.clear();
            }
            _ => {}
        }

        if matches!(
            std::env::var("ALICE_WALLET_QA_IMPORT_METHOD")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "private-key" | "private_key" | "key"
        ) {
            self.import_method = ImportMethod::PrivateKey;
        }

        if matches!(
            std::env::var("ALICE_WALLET_QA_PAGE")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "send-review" | "send_review"
        ) {
            self.page = Page::Send;
            self.send_recipient = crypto::qa_display_address();
            self.send_amount = "0".to_string();
            self.send_note = "Local review preview only".to_string();
            self.send_review_ready = true;
            self.send_review_error = None;
        }
    }

    fn enable_phase40t_evidence_mode(&mut self) {
        if self.qa_mock_mode
            || !self.network_disabled
            || !self.evidence_redact_secrets
            || !crate::config::wallet_data_root_is_overridden()
        {
            self.connection_status = ConnectionState::Error;
            self.node_sync = NodeSyncSnapshot::unavailable(
                "phase40t_evidence_requires_isolated_redacted_network_disabled",
            );
            self.sync_error =
                Some("phase40t_evidence_requires_isolated_redacted_network_disabled".to_string());
            return;
        }

        let mut seed = [0u8; 32];
        let mut pass_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        rand::thread_rng().fill_bytes(&mut pass_bytes);
        let mut seed_hex = hex::encode(seed);
        let mut password = hex::encode(pass_bytes);
        seed.zeroize();
        pass_bytes.zeroize();

        let result =
            crypto::create_wallet_payload_from_seed_hex(&seed_hex, &password).and_then(|payload| {
                let unlocked = crypto::unlock_wallet(&payload, &password)?;
                Ok((payload, unlocked.secrets))
            });
        seed_hex.zeroize();
        password.zeroize();

        let Ok((payload, secrets)) = result else {
            self.connection_status = ConnectionState::Error;
            self.node_sync = NodeSyncSnapshot::unavailable("phase40t_evidence_wallet_unavailable");
            self.sync_error = Some("phase40t_evidence_wallet_unavailable".to_string());
            return;
        };

        let profile_id = "phase40t-owner-test-evidence".to_string();
        let address = payload.address.clone();
        if self.profile_manager.profile(&profile_id).is_none() {
            let _ = self.profile_manager.register_profile(
                profile_id.clone(),
                "Owner test evidence wallet".to_string(),
                address.clone(),
                WalletProfileAccess::Normal,
            );
        } else {
            let _ = self.profile_manager.set_active_profile(&profile_id);
        }
        let contact_address = crypto::qa_display_address_variant(0x40);
        let _ = self.profile_manager.add_address_book_record(
            &profile_id,
            "Owner test local contact",
            &contact_address,
            "Local metadata only",
        );
        let _ = self.profile_manager.add_receive_request(
            &profile_id,
            "Owner test receive request",
            &address,
            Some("0 ALICE".to_string()),
        );

        self.payload = Some(payload);
        self.secrets = Some(secrets);
        self.balance = None;
        self.block_height = None;
        self.node_sync = NodeSyncSnapshot::unavailable("owner_test_network_disabled");
        self.sync_error = Some("owner_test_network_disabled".to_string());
        self.connection_status = ConnectionState::Error;
        self.refresh_pending = 0;
        self.clear_password_inputs();
        self.clear_mnemonic_inputs();
        self.clear_private_key_input();
        self.clear_private_key_export();
        self.clear_private_key_export_password();

        match std::env::var("ALICE_WALLET_PHASE40T_EVIDENCE_PAGE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "profiles" | "wallet-choice" | "wallet_choice" => {
                self.phase = Phase::WalletChoice;
                self.secrets = None;
            }
            "create" => {
                self.phase = Phase::Create;
                self.payload = None;
                self.secrets = None;
            }
            "unlock" => {
                self.phase = Phase::Unlock;
                self.secrets = None;
            }
            "import" | "import-mnemonic" | "import_mnemonic" => {
                self.phase = Phase::Import;
                self.import_method = ImportMethod::Mnemonic;
                self.payload = None;
                self.secrets = None;
            }
            "import-private-key" | "import_private_key" => {
                self.phase = Phase::Import;
                self.import_method = ImportMethod::PrivateKey;
                self.payload = None;
                self.secrets = None;
            }
            "backup" => {
                self.phase = Phase::Backup;
                self.mnemonic_backup = "phase40t evidence redacted".to_string();
            }
            _ => {
                self.phase = Phase::Main;
                self.page = phase40t_evidence_page_from_env();
            }
        }
    }

    fn check_wallet(&mut self) {
        if !self.profile_manager.safe_profiles().is_empty() {
            self.phase = Phase::WalletChoice;
            return;
        }

        let detected_path = crypto::detect_wallet_path();
        if detected_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&detected_path) {
                if let Ok(payload) = serde_json::from_str::<WalletPayload>(&content) {
                    let _ = self
                        .profile_manager
                        .upsert_detected_default_profile(payload.address.clone());
                    self.detected_wallet_path = Some(detected_path);
                    self.payload = Some(payload);
                    self.phase = Phase::WalletChoice;
                    return;
                }
            }
        }
        self.detected_wallet_path = None;
        self.wallet_path = self.default_wallet_path.clone();
        self.phase = Phase::Create;
    }

    pub fn use_detected_wallet(&mut self) {
        let selected_profile_id = self.active_profile_id().or_else(|| {
            self.profile_manager
                .safe_profiles()
                .first()
                .map(|profile| profile.profile_id.clone())
        });
        if let Some(profile_id) = selected_profile_id {
            self.select_wallet_profile(&profile_id);
            return;
        }
        if let Some(path) = &self.detected_wallet_path {
            self.wallet_path = path.clone();
            self.clear_password_inputs();
            self.auth_error.clear();
            self.phase = Phase::Unlock;
        } else {
            self.prepare_new_wallet();
        }
    }

    pub fn prepare_new_wallet(&mut self) {
        self.wallet_path = self.default_wallet_path.clone();
        self.pending_profile_reservation = None;
        self.clear_password_inputs();
        self.clear_mnemonic_inputs();
        self.clear_private_key_input();
        self.clear_private_key_export();
        self.clear_private_key_export_password();
        self.clear_mnemonic_backup();
        self.auth_error.clear();
        self.phase = Phase::Create;
    }

    pub fn prepare_import_wallet(&mut self) {
        self.wallet_path = self.default_wallet_path.clone();
        self.pending_profile_reservation = None;
        self.clear_password_inputs();
        self.clear_mnemonic_inputs();
        self.clear_private_key_input();
        self.clear_private_key_export();
        self.clear_private_key_export_password();
        self.clear_mnemonic_backup();
        self.import_method = ImportMethod::Mnemonic;
        self.auth_error.clear();
        self.phase = Phase::Import;
    }

    pub fn begin_profile_create(&mut self) -> Result<(), String> {
        let reservation = self
            .profile_manager
            .reserve_new_profile("Alice wallet", WalletProfileAccess::Normal)?;
        self.wallet_path = reservation.wallet_path.clone();
        self.pending_profile_reservation = Some(reservation);
        Ok(())
    }

    pub fn begin_profile_import(&mut self) -> Result<std::path::PathBuf, String> {
        let reservation = self
            .profile_manager
            .reserve_new_profile("Imported wallet", WalletProfileAccess::Normal)?;
        self.wallet_path = reservation.wallet_path.clone();
        self.pending_profile_reservation = Some(reservation);
        Ok(self.wallet_path.clone())
    }

    pub fn finalize_pending_profile(&mut self, address: String) -> Result<(), String> {
        let Some(reservation) = self.pending_profile_reservation.take() else {
            return Ok(());
        };
        self.profile_manager
            .finalize_reserved_profile(reservation, address)?;
        if !self.qa_mock_mode {
            self.profile_manager.save()?;
        }
        Ok(())
    }

    pub fn clear_active_wallet_secret_state(&mut self) {
        self.secrets = None;
        self.payload = None;
        self.balance = None;
        self.block_height = None;
        self.clear_password_inputs();
        self.clear_mnemonic_inputs();
        self.clear_private_key_input();
        self.clear_private_key_export();
        self.clear_private_key_export_password();
        self.clear_mnemonic_backup();
        self.reset_send_review();
        self.refresh_pending = 0;
    }

    pub fn set_page(&mut self, page: Page) {
        if self.page == Page::Accounts && page != Page::Accounts {
            self.clear_private_key_export();
            self.clear_private_key_export_password();
        }
        self.page = page;
    }

    pub fn select_wallet_profile(&mut self, profile_id: &str) {
        let Some(profile) = self.profile_manager.profile(profile_id).cloned() else {
            self.auth_error = "Wallet profile is not available.".to_string();
            return;
        };

        self.clear_active_wallet_secret_state();
        if let Err(err) = self.profile_manager.set_active_profile(&profile.profile_id) {
            self.auth_error = err;
            return;
        }

        match profile.access {
            WalletProfileAccess::ReadOnly | WalletProfileAccess::DisplayOnly => {
                self.wallet_path = self
                    .profile_manager
                    .profile_wallet_path(&profile.profile_id);
                self.secrets = Some(crypto::WalletSecrets::display_only(profile.address));
                self.phase = Phase::Main;
                self.page = Page::Dashboard;
                self.auth_error.clear();
            }
            WalletProfileAccess::Normal => {
                let path = if profile.profile_id == LEGACY_PROFILE_ID {
                    self.detected_wallet_path
                        .clone()
                        .unwrap_or_else(crypto::detect_wallet_path)
                } else {
                    self.profile_manager
                        .profile_wallet_path(&profile.profile_id)
                };
                match std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|content| serde_json::from_str::<WalletPayload>(&content).ok())
                {
                    Some(payload) => {
                        self.wallet_path = path;
                        self.payload = Some(payload);
                        self.phase = Phase::Unlock;
                        self.auth_error.clear();
                    }
                    None => {
                        self.auth_error =
                            "Wallet profile data is unavailable on this device.".to_string();
                        self.phase = Phase::WalletChoice;
                    }
                }
            }
        }
    }

    pub fn active_profile_id(&self) -> Option<String> {
        self.profile_manager
            .active_profile()
            .map(|profile| profile.profile_id.clone())
    }

    pub fn active_profile_metadata(&self) -> Option<WalletProfileMetadata> {
        self.profile_manager.active_profile().cloned()
    }

    pub fn selected_reward_identity(&self) -> Option<String> {
        wallet_profiles::selected_wallet_address(&self.profile_manager)
            .or_else(|| self.secrets.as_ref().map(|secrets| secrets.address.clone()))
    }

    pub fn active_address_book_records(&self) -> Vec<AddressBookRecord> {
        let Some(profile_id) = self.active_profile_id() else {
            return Vec::new();
        };
        self.profile_manager.address_book_records(&profile_id)
    }

    pub fn active_receive_requests(&self) -> Vec<ReceiveRequestRecord> {
        let Some(profile_id) = self.active_profile_id() else {
            return Vec::new();
        };
        self.profile_manager.receive_requests(&profile_id)
    }

    pub fn lang(&self) -> Lang {
        self.settings.lang
    }

    pub fn t(&self, key: &str) -> &'static str {
        i18n::t(self.lang(), key)
    }

    pub fn lock_now(&mut self) {
        self.secrets = None;
        self.balance = None;
        self.clear_password_inputs();
        self.clear_mnemonic_inputs();
        self.clear_private_key_input();
        self.clear_private_key_export();
        self.clear_private_key_export_password();
        self.clear_mnemonic_backup();
        self.auth_error.clear();
        self.refresh_pending = 0;
        self.phase = if self.payload.is_some() {
            Phase::Unlock
        } else {
            Phase::WalletChoice
        };
    }

    pub fn auto_lock_remaining(&self) -> Option<u64> {
        if self.settings.auto_lock_minutes == 0 {
            return None;
        }
        let limit = Duration::from_secs(self.settings.auto_lock_minutes as u64 * 60);
        let elapsed = self.last_interaction.elapsed();
        if elapsed >= limit {
            Some(0)
        } else {
            Some((limit - elapsed).as_secs())
        }
    }

    pub fn bump_interaction(&mut self) {
        self.last_interaction = Instant::now();
    }

    pub fn save_settings(&self) -> Result<(), String> {
        if self.qa_mock_mode {
            return Ok(());
        }
        self.settings.save()
    }

    // ── Embedded node management ────────────────────────────────────────────

    /// Per-OS node data directory under the wallet data root.
    pub fn node_base_path(&self) -> std::path::PathBuf {
        crate::config::wallet_data_root().join("node")
    }

    /// PID file for the embedded node.
    pub fn node_pid_path(&self) -> std::path::PathBuf {
        crate::config::wallet_data_root()
            .join("run")
            .join("node.pid")
    }

    /// Build a fully-validated launch plan for the embedded node, resolving the
    /// bundled binary + chain spec (and verifying the spec SHA if pinned).
    /// Returns a user-facing error when the binary/spec aren't bundled.
    pub fn build_node_launch(&self) -> Result<node::NodeLaunchPlan, String> {
        let program = node::resolve_node_binary()?;
        let spec = node::resolve_chain_spec()?;
        // SHA pin is None until baked into the release build (Phase 5); this is
        // the fail-closed seam, not a no-op in production.
        node::verify_chain_spec_sha256(&spec, node::pinned_chain_spec_sha256())?;
        node::build_node_launch_plan(
            program,
            &spec,
            self.node_base_path(),
            &self.settings.node,
            &node::bundled_bootnodes(),
        )
    }

    /// Non-secret environment for the node child (kept under the wallet root).
    pub fn node_child_env(&self) -> Vec<(String, String)> {
        // No secrets ever cross this boundary (plan invariant). We only scope
        // the node's data location; everything else it derives itself.
        Vec::new()
    }

    /// Kick off the embedded node (if not already active).
    pub fn start_embedded_node(&mut self) {
        if self.qa_mock_mode || self.network_disabled {
            self.toast = Some(Toast::err(
                self.t("node.start_failed_title"),
                self.t("node.unavailable_isolated"),
            ));
            return;
        }
        match self.build_node_launch() {
            Ok(plan) => {
                let envs = self.node_child_env();
                let pid = self.node_pid_path();
                let _ = self.tx.send(AsyncAction::StartNode(plan, envs, pid));
                self.node_proc.state = crate::supervise::ProcState::Starting;
            }
            Err(e) => {
                self.toast = Some(Toast::err(self.t("node.start_failed_title"), e));
            }
        }
    }

    pub fn stop_embedded_node(&mut self) {
        let _ = self.tx.send(AsyncAction::StopNode);
        self.node_proc.state = crate::supervise::ProcState::Stopping;
    }

    /// The RPC URL to use for chain queries, honoring node mode.
    pub fn effective_rpc_url(&self) -> String {
        self.settings.effective_rpc_url()
    }

    pub fn start_refresh(&mut self, address: &str) {
        if self.qa_mock_mode {
            let _ = address;
            self.balance = Some(0);
            self.node_sync = NodeSyncSnapshot::unavailable("qa_mock_local_fixture_only");
            self.connection_status = ConnectionState::Error;
            self.sync_error = Some("Mock QA mode uses local fixture data only.".to_string());
            self.refresh_pending = 0;
            return;
        }
        if self.network_disabled {
            let _ = address;
            self.balance = None;
            self.block_height = None;
            self.node_sync = NodeSyncSnapshot::unavailable("owner_test_network_disabled");
            self.connection_status = ConnectionState::Error;
            self.sync_error = Some("owner_test_network_disabled".to_string());
            self.refresh_pending = 0;
            return;
        }
        if self.settings.node.mode == NodeMode::Offline {
            self.balance = None;
            self.node_sync = NodeSyncSnapshot::unavailable("node_mode_offline");
            self.connection_status = ConnectionState::Error;
            self.sync_error = Some("node_mode_offline".to_string());
            self.refresh_pending = 0;
            return;
        }
        self.refresh_pending += 1;
        self.sync_error = None;
        let _ = self.tx.send(AsyncAction::RefreshAll(
            self.effective_rpc_url(),
            address.to_owned(),
        ));
    }

    pub fn finish_refresh(&mut self) {
        self.refresh_pending = self.refresh_pending.saturating_sub(1);
    }

    pub fn clear_password_inputs(&mut self) {
        self.password_input.zeroize();
        self.password_input.clear();
        self.confirm_password_input.zeroize();
        self.confirm_password_input.clear();
        self.password_visible = false;
    }

    pub fn clear_mnemonic_inputs(&mut self) {
        for word in &mut self.mnemonic_words {
            word.zeroize();
            word.clear();
        }
        self.mnemonic_words = vec![String::new(); 24];
    }

    pub fn clear_private_key_input(&mut self) {
        self.private_key_input.zeroize();
        self.private_key_input.clear();
    }

    pub fn clear_private_key_export(&mut self) {
        self.private_key_export.zeroize();
        self.private_key_export.clear();
    }

    pub fn clear_private_key_export_password(&mut self) {
        self.private_key_export_password.zeroize();
        self.private_key_export_password.clear();
        self.private_key_export_password_visible = false;
    }

    pub fn reveal_private_key_export(&mut self) {
        self.clear_private_key_export();
        let Some(payload) = self.payload.as_ref() else {
            self.auth_error = self.t("accounts.export_unavailable").to_string();
            return;
        };

        let mut password = std::mem::take(&mut self.private_key_export_password);
        self.private_key_export_password_visible = false;
        if password.trim().is_empty() {
            password.zeroize();
            self.auth_error = self.t("accounts.export_reauth_required").to_string();
            return;
        }

        match crypto::unlock_wallet(payload, &password) {
            Ok(unlocked) => {
                self.private_key_export = unlocked
                    .secrets
                    .export_private_key_hex()
                    .unwrap_or_default();
                self.auth_error.clear();
            }
            Err(_) => {
                self.auth_error = self.t("accounts.export_unavailable").to_string();
            }
        }
        password.zeroize();
    }

    pub fn clear_mnemonic_backup(&mut self) {
        self.mnemonic_backup.zeroize();
        self.mnemonic_backup.clear();
        for s in &mut self.backup_quiz_inputs {
            s.zeroize();
            s.clear();
        }
    }

    /// Copy `text` to clipboard and schedule it to be cleared after 30 s.
    pub fn copy_sensitive(&mut self, ctx: &eframe::egui::Context, text: &str) {
        ctx.copy_text(text.to_string());
        self.clipboard_clear_at = Some(Instant::now() + std::time::Duration::from_secs(30));
    }

    /// Tick clipboard auto-clear. Called every frame.
    pub fn tick_clipboard_clear(&mut self, ctx: &eframe::egui::Context) {
        if let Some(t) = self.clipboard_clear_at {
            if Instant::now() >= t {
                ctx.copy_text(String::new());
                self.clipboard_clear_at = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
    }

    /// Pick 3 random distinct word indices for the backup verification drill.
    pub fn pick_backup_quiz(&mut self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut picks = std::collections::BTreeSet::new();
        while picks.len() < 3 {
            picks.insert(rng.gen_range(0..24));
        }
        let v: Vec<usize> = picks.into_iter().collect();
        self.backup_quiz_indices = [v[0], v[1], v[2]];
        for s in &mut self.backup_quiz_inputs {
            s.zeroize();
            s.clear();
        }
    }

    pub fn push_history(&mut self, rec: TxRecord) {
        self.history.insert(0, rec.clone());
        if self.history.len() > 500 {
            self.history.truncate(500);
        }
        history::append(rec);
    }

    pub fn reset_send_review(&mut self) {
        self.send_review_ready = false;
        self.send_review_error = None;
    }
}

fn qa_page_from_env() -> Page {
    match std::env::var("ALICE_WALLET_QA_PAGE")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "receive" => Page::Receive,
        "send" | "send-review" | "send_review" => Page::Send,
        "mining" => Page::Mining,
        "history" => Page::History,
        "accounts" => Page::Accounts,
        "address-book" | "address_book" => Page::AddressBook,
        "settings" => Page::Settings,
        _ => Page::Dashboard,
    }
}

fn phase40t_evidence_page_from_env() -> Page {
    match std::env::var("ALICE_WALLET_PHASE40T_EVIDENCE_PAGE")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "receive" => Page::Receive,
        "send" => Page::Send,
        "mining" => Page::Mining,
        "history" => Page::History,
        "accounts" => Page::Accounts,
        "address-book" | "address_book" => Page::AddressBook,
        "settings" => Page::Settings,
        _ => Page::Dashboard,
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn phase40t_temp_root(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "alice-wallet-phase40t-{}-{}-{}",
            label,
            std::process::id(),
            stamp
        ))
    }

    #[test]
    fn qa_mock_mode_uses_display_only_wallet_and_skips_network_refresh() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("ALICE_WALLET_QA_MOCK", "1");
        let rt = Runtime::new().expect("runtime");
        let mut app = AliceWalletApp::new(rt);
        std::env::remove_var("ALICE_WALLET_QA_MOCK");

        assert!(app.qa_mock_mode);
        assert_eq!(app.phase, Phase::Main);
        assert!(app.payload.is_none());
        assert!(app.secrets.is_some());
        assert_eq!(app.balance, Some(0));

        app.start_refresh("ignored-in-qa-mode");
        assert_eq!(app.refresh_pending, 0);
        assert!(matches!(app.connection_status, ConnectionState::Error));
        assert!(app
            .sync_error
            .as_deref()
            .unwrap_or_default()
            .contains("local fixture"));
    }

    #[test]
    fn qa_mock_mode_can_open_pages_and_auth_flows_without_wallet_data() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("ALICE_WALLET_QA_MOCK", "1");
        std::env::set_var("ALICE_WALLET_QA_PAGE", "mining");
        let rt = Runtime::new().expect("runtime");
        let app = AliceWalletApp::new(rt);
        assert_eq!(app.phase, Phase::Main);
        assert_eq!(app.page, Page::Mining);
        assert!(app.payload.is_none());

        std::env::set_var("ALICE_WALLET_QA_PHASE", "import");
        std::env::remove_var("ALICE_WALLET_QA_PAGE");
        let rt = Runtime::new().expect("runtime");
        let app = AliceWalletApp::new(rt);
        assert_eq!(app.phase, Phase::Import);
        assert!(app.payload.is_none());
        assert!(app.secrets.is_none());

        std::env::set_var("ALICE_WALLET_QA_PHASE", "");
        std::env::set_var("ALICE_WALLET_QA_PAGE", "send-review");
        let rt = Runtime::new().expect("runtime");
        let app = AliceWalletApp::new(rt);
        assert_eq!(app.phase, Phase::Main);
        assert_eq!(app.page, Page::Send);
        assert!(app.send_review_ready);
        assert_eq!(app.send_amount, "0");

        std::env::remove_var("ALICE_WALLET_QA_MOCK");
        std::env::remove_var("ALICE_WALLET_QA_PAGE");
        std::env::remove_var("ALICE_WALLET_QA_PHASE");
    }

    #[test]
    fn owner_test_data_root_launches_without_qa_mock_and_stays_isolated() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = phase40t_temp_root("data-root");
        std::env::remove_var("ALICE_WALLET_QA_MOCK");
        std::env::set_var("ALICE_WALLET_DATA_ROOT", &root);
        std::env::set_var("ALICE_WALLET_NETWORK_DISABLED", "1");

        let rt = Runtime::new().expect("runtime");
        let app = AliceWalletApp::new(rt);

        std::env::remove_var("ALICE_WALLET_DATA_ROOT");
        std::env::remove_var("ALICE_WALLET_NETWORK_DISABLED");
        let _ = std::fs::remove_dir_all(&root);

        assert!(!app.qa_mock_mode);
        assert!(app.network_disabled);
        assert_eq!(app.wallet_path, root.join("wallet.json"));
        assert_eq!(app.default_wallet_path, root.join("wallet.json"));
        assert_eq!(app.phase, Phase::Create);
    }

    #[test]
    fn owner_test_network_disabled_fails_closed_without_rpc_refresh() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = phase40t_temp_root("network-disabled");
        std::env::remove_var("ALICE_WALLET_QA_MOCK");
        std::env::set_var("ALICE_WALLET_DATA_ROOT", &root);
        std::env::set_var("ALICE_WALLET_NETWORK_DISABLED", "1");

        let rt = Runtime::new().expect("runtime");
        let mut app = AliceWalletApp::new(rt);
        app.start_refresh("owner-test-address");

        std::env::remove_var("ALICE_WALLET_DATA_ROOT");
        std::env::remove_var("ALICE_WALLET_NETWORK_DISABLED");
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(app.refresh_pending, 0);
        assert!(matches!(app.connection_status, ConnectionState::Error));
        assert_eq!(
            app.node_sync.fail_closed_reason.as_deref(),
            Some("owner_test_network_disabled")
        );
    }

    #[test]
    fn phase40t_evidence_mode_is_no_mock_isolated_redacted_and_fail_closed() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = phase40t_temp_root("evidence-mode");
        std::env::remove_var("ALICE_WALLET_QA_MOCK");
        std::env::set_var("ALICE_WALLET_DATA_ROOT", &root);
        std::env::set_var("ALICE_WALLET_NETWORK_DISABLED", "1");
        std::env::set_var("ALICE_WALLET_EVIDENCE_REDACT_SECRETS", "1");
        std::env::set_var("ALICE_WALLET_PHASE40T_EVIDENCE_PAGE", "accounts");

        let rt = Runtime::new().expect("runtime");
        let app = AliceWalletApp::new(rt);

        std::env::remove_var("ALICE_WALLET_DATA_ROOT");
        std::env::remove_var("ALICE_WALLET_NETWORK_DISABLED");
        std::env::remove_var("ALICE_WALLET_EVIDENCE_REDACT_SECRETS");
        std::env::remove_var("ALICE_WALLET_PHASE40T_EVIDENCE_PAGE");
        let _ = std::fs::remove_dir_all(&root);

        assert!(!app.qa_mock_mode);
        assert!(app.network_disabled);
        assert!(app.evidence_redact_secrets);
        assert_eq!(app.phase, Phase::Main);
        assert_eq!(app.page, Page::Accounts);
        assert!(app.payload.is_some());
        assert!(app
            .secrets
            .as_ref()
            .is_some_and(|wallet| wallet.can_export_private_key()));
        assert_eq!(
            app.node_sync.fail_closed_reason.as_deref(),
            Some("owner_test_network_disabled")
        );
        assert!(matches!(app.connection_status, ConnectionState::Error));
    }

    #[test]
    fn private_key_export_reauth_derives_from_payload_and_clears_on_page_leave() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = phase40t_temp_root("private-key-export");
        std::env::remove_var("ALICE_WALLET_QA_MOCK");
        std::env::set_var("ALICE_WALLET_DATA_ROOT", &root);
        std::env::set_var("ALICE_WALLET_NETWORK_DISABLED", "1");

        let rt = Runtime::new().expect("runtime");
        let mut app = AliceWalletApp::new(rt);
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        let mut seed_hex = hex::encode(seed);
        let password = format!("phase40t-{}-{}", std::process::id(), root.display());
        let payload =
            crypto::create_wallet_payload_from_seed_hex(&seed_hex, &password).expect("payload");
        seed.zeroize();
        seed_hex.zeroize();

        app.payload = Some(payload);
        app.page = Page::Accounts;
        app.private_key_export_password = password;
        app.reveal_private_key_export();

        assert!(app.private_key_export.starts_with("0x"));
        assert!(app.private_key_export_password.is_empty());

        app.set_page(Page::Dashboard);
        assert!(app.private_key_export.is_empty());
        assert!(app.private_key_export_password.is_empty());

        std::env::remove_var("ALICE_WALLET_DATA_ROOT");
        std::env::remove_var("ALICE_WALLET_NETWORK_DISABLED");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn push_history_caps_memory_and_persists_local_history() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = phase40t_temp_root("history-persistence");
        std::env::remove_var("ALICE_WALLET_QA_MOCK");
        std::env::set_var("ALICE_WALLET_DATA_ROOT", &root);
        std::env::set_var("ALICE_WALLET_NETWORK_DISABLED", "1");

        let rt = Runtime::new().expect("runtime");
        let mut app = AliceWalletApp::new(rt);
        app.history.clear();

        for index in 0..505 {
            app.push_history(TxRecord {
                ts: chrono::Utc::now(),
                kind: history::TxKind::Send,
                amount: Some(index),
                counterparty: Some(format!("counterparty-{index}")),
                hash: format!("0x{index:064x}"),
                ok: true,
            });
        }

        let persisted = history::load();

        std::env::remove_var("ALICE_WALLET_DATA_ROOT");
        std::env::remove_var("ALICE_WALLET_NETWORK_DISABLED");
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(app.history.len(), 500);
        assert_eq!(persisted.len(), 500);
        assert_eq!(
            persisted.first().and_then(|record| record.amount),
            Some(504)
        );
        assert_eq!(persisted.last().and_then(|record| record.amount), Some(5));
    }

    #[test]
    fn phase40t_materializes_owner_test_profiles_when_requested() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let Some(root) = std::env::var_os("ALICE_WALLET_PHASE40T_MATERIALIZE_ROOT")
            .map(std::path::PathBuf::from)
        else {
            return;
        };

        let mut manager = WalletProfileManager::new(root.clone());
        let mut profile_ids = Vec::new();
        let mut addresses = Vec::new();
        for label in ["Owner test primary", "Owner test imported"] {
            let reservation = manager
                .reserve_new_profile(label, WalletProfileAccess::Normal)
                .expect("reserve profile");
            let mut seed = [0u8; 32];
            let mut pass_bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut seed);
            rand::thread_rng().fill_bytes(&mut pass_bytes);
            let mut seed_hex = hex::encode(seed);
            let mut password = hex::encode(pass_bytes);
            seed.zeroize();
            pass_bytes.zeroize();
            let payload =
                crypto::create_wallet_payload_from_seed_hex(&seed_hex, &password).expect("payload");
            seed_hex.zeroize();
            password.zeroize();
            crypto::write_wallet_payload(&reservation.wallet_path, &payload)
                .expect("write payload");
            let profile_id = reservation.profile_id.clone();
            let address = payload.address.clone();
            manager
                .finalize_reserved_profile(reservation, address.clone())
                .expect("finalize profile");
            profile_ids.push(profile_id);
            addresses.push(address);
        }

        if profile_ids.len() == 2 && addresses.len() == 2 {
            manager
                .add_address_book_record(
                    &profile_ids[0],
                    "Owner test local contact",
                    &addresses[1],
                    "local metadata only",
                )
                .expect("address book");
            manager
                .add_receive_request(
                    &profile_ids[0],
                    "Owner test receive request",
                    &addresses[0],
                    Some("0 ALICE".to_string()),
                )
                .expect("receive request");
            manager
                .set_active_profile(&profile_ids[0])
                .expect("active profile");
        }

        manager.save().expect("save profile metadata");
    }

    #[test]
    fn qa_mock_mode_exposes_two_display_only_profiles() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("ALICE_WALLET_QA_MOCK", "1");
        let rt = Runtime::new().expect("runtime");
        let app = AliceWalletApp::new(rt);
        std::env::remove_var("ALICE_WALLET_QA_MOCK");

        let profiles = app.profile_manager.safe_profiles();
        assert!(profiles.len() >= 2);
        assert!(profiles
            .iter()
            .all(|profile| profile.access == WalletProfileAccess::DisplayOnly));
        assert!(app.secrets.is_some());
        assert!(app.payload.is_none());
    }

    #[test]
    fn switching_active_profile_clears_in_memory_secrets() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::set_var("ALICE_WALLET_QA_MOCK", "1");
        let rt = Runtime::new().expect("runtime");
        let mut app = AliceWalletApp::new(rt);
        std::env::remove_var("ALICE_WALLET_QA_MOCK");

        app.password_input = "temporary-passphrase".to_string();
        app.mnemonic_backup = "temporary local backup phrase".to_string();
        app.balance = Some(99);
        let first_address = app.secrets.as_ref().map(|secrets| secrets.address.clone());
        app.select_wallet_profile("qa-cold-wallet");

        assert!(app.password_input.is_empty());
        assert!(app.mnemonic_backup.is_empty());
        assert_eq!(app.balance, None);
        assert_eq!(app.active_profile_id().as_deref(), Some("qa-cold-wallet"));
        assert_ne!(
            first_address,
            app.secrets.as_ref().map(|secrets| secrets.address.clone())
        );
    }
}

fn spawn_worker(
    rt: Arc<Runtime>,
    rx: Receiver<AsyncAction>,
    tx: Sender<AsyncResult>,
    node_supervisor: NodeSupervisor,
) {
    std::thread::spawn(move || {
        while let Ok(action) = rx.recv() {
            match action {
                AsyncAction::StartNode(plan, envs, pid_file) => {
                    // Spawn must run on the tokio runtime (it spawns child I/O
                    // tasks). Enter the runtime context, then report status.
                    let sup = node_supervisor.clone();
                    let tx = tx.clone();
                    let _guard = rt.enter();
                    match sup.start(plan, envs, Some(pid_file), true) {
                        Ok(()) => {
                            let _ = tx.send(AsyncResult::NodeProc(sup.status()));
                        }
                        Err(e) => {
                            let _ = tx.send(AsyncResult::NodeProcErr(e));
                            let _ = tx.send(AsyncResult::NodeProc(sup.status()));
                        }
                    }
                }
                AsyncAction::StopNode => {
                    node_supervisor.request_stop();
                    let _ = tx.send(AsyncResult::NodeProc(node_supervisor.status()));
                }
                AsyncAction::PollNodeProc => {
                    let _ = tx.send(AsyncResult::NodeProc(node_supervisor.status()));
                }
                AsyncAction::Unlock(payload, mut password) => {
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        match crypto::unlock_wallet(&payload, &password) {
                            Ok(u) => {
                                password.zeroize();
                                let _ =
                                    tx.send(AsyncResult::UnlockOk(u.secrets, u.upgraded_payload));
                            }
                            Err(e) => {
                                let _ = tx.send(AsyncResult::UnlockErr(e));
                            }
                        }
                        password.zeroize();
                    });
                }
                AsyncAction::Create(mut phrase, mut password) => {
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        match crypto::create_wallet_payload(&phrase, &password) {
                            Ok(payload) => match crypto::unlock_wallet(&payload, &password) {
                                Ok(unlocked) => {
                                    let phrase_for_ui = std::mem::take(&mut phrase);
                                    password.zeroize();
                                    let _ = tx.send(AsyncResult::CreateOk(
                                        payload,
                                        unlocked.secrets,
                                        phrase_for_ui,
                                    ));
                                }
                                Err(e) => {
                                    let _ = tx.send(AsyncResult::CreateErr(e));
                                }
                            },
                            Err(e) => {
                                let _ = tx.send(AsyncResult::CreateErr(e));
                            }
                        }
                        phrase.zeroize();
                        password.zeroize();
                    });
                }
                AsyncAction::Import(mut phrase, mut password, target_path) => {
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        // Safety: backup any existing wallet before overwrite.
                        let backup_result = crypto::backup_existing_wallet(&target_path);
                        let backed_up = match backup_result {
                            Ok(p) => p,
                            Err(e) => {
                                let _ = e;
                                let _ = tx.send(AsyncResult::CreateErr(
                                    "Could not prepare the existing wallet safely. Try again before importing.".into(),
                                ));
                                phrase.zeroize();
                                password.zeroize();
                                return;
                            }
                        };
                        match crypto::create_wallet_payload(&phrase, &password) {
                            Ok(payload) => match crypto::unlock_wallet(&payload, &password) {
                                Ok(unlocked) => {
                                    password.zeroize();
                                    let _ = tx.send(AsyncResult::ImportOk(
                                        payload,
                                        unlocked.secrets,
                                        backed_up,
                                    ));
                                }
                                Err(e) => {
                                    let _ = tx.send(AsyncResult::CreateErr(e));
                                }
                            },
                            Err(e) => {
                                let _ = tx.send(AsyncResult::CreateErr(e));
                            }
                        }
                        phrase.zeroize();
                        password.zeroize();
                    });
                }
                AsyncAction::ImportSeedHex(mut seed_hex, mut password, target_path) => {
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        // Safety: backup any existing wallet before overwrite.
                        let backup_result = crypto::backup_existing_wallet(&target_path);
                        let backed_up = match backup_result {
                            Ok(p) => p,
                            Err(e) => {
                                let _ = e;
                                let _ = tx.send(AsyncResult::CreateErr(
                                    "Could not prepare the existing wallet safely. Try again before importing.".into(),
                                ));
                                seed_hex.zeroize();
                                password.zeroize();
                                return;
                            }
                        };
                        match crypto::create_wallet_payload_from_seed_hex(&seed_hex, &password) {
                            Ok(payload) => match crypto::unlock_wallet(&payload, &password) {
                                Ok(unlocked) => {
                                    password.zeroize();
                                    let _ = tx.send(AsyncResult::ImportOk(
                                        payload,
                                        unlocked.secrets,
                                        backed_up,
                                    ));
                                }
                                Err(e) => {
                                    let _ = tx.send(AsyncResult::CreateErr(e));
                                }
                            },
                            Err(e) => {
                                let _ = e;
                                let _ = tx.send(AsyncResult::CreateErr(
                                    "Private key could not be imported. Check the format and try again.".into(),
                                ));
                            }
                        }
                        seed_hex.zeroize();
                        password.zeroize();
                    });
                }
                AsyncAction::RefreshAll(url, address) => {
                    let tx = tx.clone();
                    rt.spawn(async move {
                        let fut = async {
                            let snapshot = chain::fetch_node_sync_snapshot(&url).await;
                            let allows_balance_refresh = snapshot.allows_balance_refresh();
                            let fail_closed_reason = snapshot
                                .fail_closed_reason
                                .clone()
                                .unwrap_or_else(|| "node_not_ready_for_balance_refresh".into());
                            let _ = tx.send(AsyncResult::NodeSync(snapshot));
                            if !allows_balance_refresh {
                                let _ = tx.send(AsyncResult::SyncErr(format!(
                                    "Balance blocked: {}",
                                    fail_closed_reason
                                )));
                                return;
                            }
                            match chain::get_client(&url).await {
                                Ok(client) => {
                                    let _ = tx.send(AsyncResult::ConnectionOk);
                                    match chain::get_balance(&client, &address).await {
                                        Ok(b) => {
                                            let _ = tx.send(AsyncResult::Balance(b));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(AsyncResult::SyncErr(format!(
                                                "Balance: {}",
                                                e
                                            )));
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(AsyncResult::ConnectionErr(e));
                                }
                            }
                        };
                        if tokio::time::timeout(Duration::from_secs(12), fut)
                            .await
                            .is_err()
                        {
                            let _ = tx.send(AsyncResult::ConnectionErr(
                                "RPC connection timed out".into(),
                            ));
                        }
                    });
                }
                AsyncAction::RefreshNodeSync(url) => {
                    let tx = tx.clone();
                    rt.spawn(async move {
                        let fut = chain::fetch_node_sync_snapshot(&url);
                        match tokio::time::timeout(Duration::from_secs(8), fut).await {
                            Ok(snapshot) => {
                                let _ = tx.send(AsyncResult::NodeSync(snapshot));
                            }
                            Err(_) => {
                                let _ = tx.send(AsyncResult::NodeSync(
                                    NodeSyncSnapshot::unavailable("node_status_timeout"),
                                ));
                            }
                        }
                    });
                }
            }
        }
    });
}

// ────────────────────────────────────────────────────────────────────────────
// eframe impl
// ────────────────────────────────────────────────────────────────────────────

impl eframe::App for AliceWalletApp {
    fn ui(&mut self, ui_root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui_root.ctx().clone();
        let ctx = &ctx;
        ui::theme::apply_style(ctx);

        // Sensitive clipboard auto-clear tick
        self.tick_clipboard_clear(ctx);

        // Drain async results
        while let Ok(result) = self.rx.try_recv() {
            self.handle_async_result(result);
        }

        // Handle auto lock + pre-lock warning
        if self.phase == Phase::Main && self.secrets.is_some() {
            if let Some(remaining) = self.auto_lock_remaining() {
                if remaining == 0 {
                    self.lock_now();
                    let title = self.t("toast.locked_title").to_string();
                    let body = self.t("toast.locked_body").to_string();
                    self.toast = Some(Toast::ok(title, body));
                    self.lock_warn_shown = false;
                } else if remaining <= 30 && !self.lock_warn_shown {
                    self.lock_warn_shown = true;
                    let title = self.t("toast.lock_warn").to_string();
                    let body = format!("{}: {}s", self.t("shell.auto_lock_in"), remaining);
                    self.toast = Some(Toast::err(title, body));
                } else if remaining > 60 {
                    self.lock_warn_shown = false;
                }
            }
        }

        // Auto-refresh balance + stake every 30s on Main phase
        if self.phase == Phase::Main
            && !self.qa_mock_mode
            && !self.network_disabled
            && self.secrets.is_some()
            && self.refresh_pending == 0
        {
            let needs = self
                .last_data_poll
                .map(|t| t.elapsed() > Duration::from_secs(30))
                .unwrap_or(true);
            if needs {
                self.last_data_poll = Some(Instant::now());
                if let Some(s) = self.secrets.clone() {
                    self.start_refresh(&s.address);
                }
            }
        }

        // Esc closes review modals
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if !self.busy {}
        }

        // Background block poll when on main phase
        if self.phase == Phase::Main
            && !self.qa_mock_mode
            && !self.network_disabled
            && self.settings.node.mode != NodeMode::Offline
        {
            let needs_poll = self
                .last_block_poll
                .map(|t| t.elapsed() > Duration::from_secs(8))
                .unwrap_or(true);
            if needs_poll {
                self.last_block_poll = Some(Instant::now());
                let _ = self
                    .tx
                    .send(AsyncAction::RefreshNodeSync(self.effective_rpc_url()));
            }
        }

        // Poll embedded-node process status when managing a local node.
        if self.phase == Phase::Main
            && !self.qa_mock_mode
            && self.settings.node.mode == NodeMode::LocalEmbedded
        {
            let needs = self
                .last_node_proc_poll
                .map(|t| t.elapsed() > Duration::from_secs(2))
                .unwrap_or(true);
            if needs {
                self.last_node_proc_poll = Some(Instant::now());
                let _ = self.tx.send(AsyncAction::PollNodeProc);
            }
        }

        // Detect interaction to reset auto-lock timer
        if ctx.input(|i| i.pointer.any_pressed() || !i.events.is_empty()) {
            self.bump_interaction();
        }

        match self.phase {
            Phase::CheckWallet => self.render_loading(ui_root),
            Phase::WalletChoice => ui::unlock::render_choice(ui_root, self),
            Phase::Unlock => ui::unlock::render_unlock(ui_root, self),
            Phase::Create => ui::create::render(ui_root, self),
            Phase::Import => ui::import::render(ui_root, self),
            Phase::Backup => ui::backup::render(ui_root, self),
            Phase::Main => ui::shell::render(ui_root, self),
        }

        if self.busy || self.auth_busy || self.refresh_pending > 0 {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    /// On app shutdown, tear down the embedded node child so it never outlives
    /// the wallet (plan §1.2 "App shutdown"). Best-effort + bounded: we request
    /// a graceful stop and give the supervision loop a brief window to act;
    /// `kill_on_drop` on the child is the backstop. A node crash/teardown never
    /// touches wallet custody state.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.node_supervisor.is_active() {
            self.node_supervisor.request_stop();
            // Brief, bounded wait for graceful SIGTERM teardown.
            let deadline = Instant::now() + Duration::from_secs(6);
            while Instant::now() < deadline && self.node_supervisor.is_active() {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

impl AliceWalletApp {
    fn render_loading(&mut self, ui_root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui::theme::THEME.bg_base))
            .show_inside(ui_root, |ui| {
                let rect = ui.max_rect();
                ui::theme::paint_backdrop(ui, rect);
                ui.vertical_centered(|ui| {
                    ui.add_space(rect.height() * 0.4);
                    ui.spinner();
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Loading Alice Wallet…")
                            .size(13.0)
                            .color(ui::theme::THEME.text_mid),
                    );
                });
            });
    }

    fn handle_async_result(&mut self, result: AsyncResult) {
        match result {
            AsyncResult::UnlockOk(secrets, upgraded_payload) => {
                self.auth_busy = false;
                self.unlock_fail_count = 0;
                self.unlock_block_until = None;
                self.clear_password_inputs();
                if let Some(payload) = upgraded_payload {
                    if let Err(e) = crypto::write_wallet_payload(&self.wallet_path, &payload) {
                        let _ = e;
                        self.sync_error = Some("wallet_local_data_update_retry".to_string());
                    }
                    self.payload = Some(payload);
                }
                self.secrets = Some(secrets.clone());
                self.phase = Phase::Main;
                self.page = Page::Dashboard;
                self.auth_error.clear();
                if let Some(profile_id) = self.active_profile_id() {
                    let _ = self.profile_manager.mark_opened(&profile_id);
                    if !self.qa_mock_mode {
                        let _ = self.profile_manager.save();
                    }
                }
                self.bump_interaction();
                self.start_refresh(&secrets.address);
            }
            AsyncResult::UnlockErr(err) => {
                self.auth_busy = false;
                self.unlock_fail_count += 1;
                let delay = Duration::from_millis(500 * (1 << self.unlock_fail_count.min(4)));
                self.unlock_block_until = Some(Instant::now() + delay);
                self.auth_error =
                    format!("{} — wait {}s before retrying", err, delay.as_secs().max(1));
            }
            AsyncResult::CreateOk(payload, secrets, phrase) => {
                self.auth_busy = false;
                self.clear_password_inputs();
                let save_result = crypto::write_wallet_payload(&self.wallet_path, &payload);
                self.payload = Some(payload);
                self.secrets = Some(secrets.clone());
                self.mnemonic_backup = phrase;
                self.pick_backup_quiz();
                self.phase = Phase::Backup;
                self.auth_error = match save_result {
                    Ok(_) => match self.finalize_pending_profile(secrets.address.clone()) {
                        Ok(_) => String::new(),
                        Err(e) => e,
                    },
                    Err(e) => {
                        let _ = e;
                        "Wallet created, but saving needs retry. Keep this phrase safe.".to_string()
                    }
                };
            }
            AsyncResult::ImportOk(payload, secrets, backed_up) => {
                self.auth_busy = false;
                let save_result = crypto::write_wallet_payload(&self.wallet_path, &payload);
                self.payload = Some(payload);
                self.secrets = Some(secrets.clone());
                self.clear_password_inputs();
                self.clear_mnemonic_inputs();
                self.clear_private_key_input();
                self.clear_private_key_export();
                self.clear_private_key_export_password();
                self.clear_mnemonic_backup();
                match save_result {
                    Ok(_) => {
                        if let Err(err) = self.finalize_pending_profile(secrets.address.clone()) {
                            self.phase = Phase::Import;
                            self.auth_error = err;
                            return;
                        }
                        self.phase = Phase::Main;
                        self.page = Page::Dashboard;
                        self.auth_error.clear();
                        self.bump_interaction();
                        self.start_refresh(&secrets.address);
                        if let Some(path) = backed_up {
                            let _ = path;
                            self.toast = Some(Toast::ok(
                                self.t("toast.backed_up"),
                                self.t("toast.backed_up_body"),
                            ));
                        }
                    }
                    Err(e) => {
                        let _ = e;
                        self.phase = Phase::Import;
                        self.auth_error = self.t("auth.import_save_failed").to_string();
                    }
                }
            }
            AsyncResult::CreateErr(err) => {
                self.auth_busy = false;
                self.auth_error = err;
            }
            AsyncResult::Balance(b) => {
                self.finish_refresh();
                self.balance = Some(b);
                self.connection_status = ConnectionState::Connected;
            }
            AsyncResult::NodeSync(snapshot) => {
                self.block_height = snapshot.current_height;
                self.connection_status = match snapshot.status {
                    NodeSyncState::Synced | NodeSyncState::Syncing => ConnectionState::Connected,
                    NodeSyncState::Stale
                    | NodeSyncState::Offline
                    | NodeSyncState::Unavailable
                    | NodeSyncState::Error => ConnectionState::Error,
                };
                self.sync_error = snapshot.fail_closed_reason.clone();
                self.node_sync = snapshot;
            }
            AsyncResult::ConnectionOk => {
                let was_error = matches!(self.connection_status, ConnectionState::Error);
                self.connection_status = ConnectionState::Connected;
                if was_error {
                    self.toast = Some(Toast::ok(
                        self.t("toast.connected"),
                        self.t("toast.connection_back"),
                    ));
                }
            }
            AsyncResult::ConnectionErr(e) => {
                let was_ok = !matches!(self.connection_status, ConnectionState::Error);
                self.connection_status = ConnectionState::Error;
                self.sync_error = Some(e);
                self.refresh_pending = 0;
                if was_ok {
                    self.toast = Some(Toast::err(
                        self.t("toast.disconnected"),
                        self.t("toast.connection_lost"),
                    ));
                }
            }
            AsyncResult::SyncErr(err) => {
                self.finish_refresh();
                self.sync_error = Some(err);
            }
            AsyncResult::NodeProc(status) => {
                self.node_proc = status;
            }
            AsyncResult::NodeProcErr(err) => {
                self.toast = Some(Toast::err(self.t("node.start_failed_title"), err));
            }
        }
    }
}
