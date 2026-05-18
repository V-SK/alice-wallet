use crate::chain::{self, NodeSyncSnapshot, NodeSyncState};
use crate::config::{Lang, Settings};
use crate::crypto::{self, WalletPayload, WalletSecrets};
use crate::history::{self, TxRecord};
use crate::i18n;
use crate::ui;
use eframe::egui;
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
}

// ────────────────────────────────────────────────────────────────────────────

pub struct AliceWalletApp {
    pub rt: Arc<Runtime>,
    pub qa_mock_mode: bool,

    pub phase: Phase,
    pub page: Page,

    // wallet
    pub default_wallet_path: std::path::PathBuf,
    pub detected_wallet_path: Option<std::path::PathBuf>,
    pub wallet_path: std::path::PathBuf,
    pub payload: Option<WalletPayload>,
    pub secrets: Option<WalletSecrets>,

    // settings
    pub settings: Settings,
    pub settings_lock_draft: String,

    // auth inputs
    pub password_input: String,
    pub password_visible: bool,
    pub confirm_password_input: String,
    pub mnemonic_words: Vec<String>,
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

    // async
    pub tx: Sender<AsyncAction>,
    pub rx: Receiver<AsyncResult>,
}

impl AliceWalletApp {
    pub fn new(rt: Runtime) -> Self {
        let qa_mock_mode = std::env::var("ALICE_WALLET_QA_MOCK")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
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
        let (gui_tx, worker_rx) = channel::<AsyncAction>();
        let (worker_tx, gui_rx) = channel::<AsyncResult>();

        let rt = Arc::new(rt);
        spawn_worker(rt.clone(), worker_rx, worker_tx);

        let mut app = Self {
            rt,
            qa_mock_mode,
            phase: Phase::CheckWallet,
            page: Page::Dashboard,
            default_wallet_path: default_wallet_path.clone(),
            detected_wallet_path: None,
            wallet_path: default_wallet_path,
            payload: None,
            secrets: None,
            settings_lock_draft: settings.auto_lock_minutes.to_string(),
            settings,
            password_input: String::new(),
            password_visible: false,
            confirm_password_input: String::new(),
            mnemonic_words: vec![String::new(); 24],
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
            tx: gui_tx,
            rx: gui_rx,
        };

        if app.qa_mock_mode {
            app.enable_qa_mock_mode();
        } else {
            app.check_wallet();
        }
        app
    }

    fn enable_qa_mock_mode(&mut self) {
        self.phase = Phase::Main;
        self.page = qa_page_from_env();
        self.payload = None;
        self.detected_wallet_path = None;
        self.secrets = Some(crypto::WalletSecrets::display_only(
            crypto::qa_display_address(),
        ));
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

    fn check_wallet(&mut self) {
        let detected_path = crypto::detect_wallet_path();
        if detected_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&detected_path) {
                if let Ok(payload) = serde_json::from_str::<WalletPayload>(&content) {
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
        self.clear_password_inputs();
        self.clear_mnemonic_inputs();
        self.clear_mnemonic_backup();
        self.auth_error.clear();
        self.phase = Phase::Create;
    }

    pub fn prepare_import_wallet(&mut self) {
        self.wallet_path = self.default_wallet_path.clone();
        self.clear_password_inputs();
        self.clear_mnemonic_inputs();
        self.clear_mnemonic_backup();
        self.auth_error.clear();
        self.phase = Phase::Import;
    }

    pub fn lang(&self) -> Lang {
        self.settings.lang
    }

    pub fn t(&self, key: &str) -> &'static str {
        i18n::t(self.settings.lang, key)
    }

    pub fn lock_now(&mut self) {
        self.secrets = None;
        self.balance = None;
        self.clear_password_inputs();
        self.clear_mnemonic_inputs();
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
        self.refresh_pending += 1;
        self.sync_error = None;
        let _ = self.tx.send(AsyncAction::RefreshAll(
            self.settings.rpc_url.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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
}

fn spawn_worker(rt: Arc<Runtime>, rx: Receiver<AsyncAction>, tx: Sender<AsyncResult>) {
    std::thread::spawn(move || {
        while let Ok(action) = rx.recv() {
            match action {
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
                AsyncAction::RefreshAll(url, address) => {
                    let tx = tx.clone();
                    rt.spawn(async move {
                        let fut = async {
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
                                    let snapshot = chain::fetch_node_sync_snapshot(&url).await;
                                    let _ = tx.send(AsyncResult::NodeSync(snapshot));
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
        if self.phase == Phase::Main && !self.qa_mock_mode {
            let needs_poll = self
                .last_block_poll
                .map(|t| t.elapsed() > Duration::from_secs(8))
                .unwrap_or(true);
            if needs_poll {
                self.last_block_poll = Some(Instant::now());
                let _ = self
                    .tx
                    .send(AsyncAction::RefreshNodeSync(self.settings.rpc_url.clone()));
            }
        }

        // Detect interaction to reset auto-lock timer
        if ctx.input(|i| i.pointer.any_pressed() || !i.events.is_empty()) {
            self.bump_interaction();
        }

        match self.phase {
            Phase::CheckWallet => self.render_loading(ctx),
            Phase::WalletChoice => ui::unlock::render_choice(ctx, self),
            Phase::Unlock => ui::unlock::render_unlock(ctx, self),
            Phase::Create => ui::create::render(ctx, self),
            Phase::Import => ui::import::render(ctx, self),
            Phase::Backup => ui::backup::render(ctx, self),
            Phase::Main => ui::shell::render(ctx, self),
        }

        if self.busy || self.auth_busy || self.refresh_pending > 0 {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

impl AliceWalletApp {
    fn render_loading(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui::theme::THEME.bg_base))
            .show(ctx, |ui| {
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
                self.secrets = Some(secrets);
                self.mnemonic_backup = phrase;
                self.pick_backup_quiz();
                self.phase = Phase::Backup;
                self.auth_error = match save_result {
                    Ok(_) => String::new(),
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
                self.clear_mnemonic_backup();
                match save_result {
                    Ok(_) => {
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
        }
    }
}
