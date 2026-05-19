use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_RPC_URL: &str = "wss://rpc.aliceprotocol.org";
pub const DEFAULT_AUTO_LOCK_MINUTES: u32 = 10;
pub const DATA_ROOT_ENV: &str = "ALICE_WALLET_DATA_ROOT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Zh,
}

impl Default for Lang {
    fn default() -> Self {
        Lang::En
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_rpc")]
    pub rpc_url: String,
    #[serde(default = "default_lock")]
    pub auto_lock_minutes: u32,
    #[serde(default)]
    pub lang: Lang,
}

fn default_rpc() -> String {
    DEFAULT_RPC_URL.to_string()
}
fn default_lock() -> u32 {
    DEFAULT_AUTO_LOCK_MINUTES
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rpc_url: default_rpc(),
            auto_lock_minutes: default_lock(),
            lang: Lang::default(),
        }
    }
}

pub fn config_path() -> PathBuf {
    wallet_data_root().join("config.json")
}

pub fn wallet_data_root() -> PathBuf {
    if let Some(root) = wallet_data_root_override() {
        return root;
    }

    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".alice"))
        .join("AliceWallet")
}

pub fn wallet_data_root_is_overridden() -> bool {
    wallet_data_root_override().is_some()
}

fn wallet_data_root_override() -> Option<PathBuf> {
    std::env::var(DATA_ROOT_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

impl Settings {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<Settings>(&data) {
                return s;
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, data).map_err(|e| e.to_string())
    }
}
