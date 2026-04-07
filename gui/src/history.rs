use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TxKind {
    Send,
    StakeScorer,
    StakeAggregator,
    UnstakeScorer,
    UnstakeAggregator,
}

impl TxKind {
    pub fn label(&self) -> &'static str {
        match self {
            TxKind::Send => "Send",
            TxKind::StakeScorer => "Stake · Scorer",
            TxKind::StakeAggregator => "Stake · Agg",
            TxKind::UnstakeScorer => "Unstake · Scorer",
            TxKind::UnstakeAggregator => "Unstake · Agg",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    pub ts: DateTime<Utc>,
    pub kind: TxKind,
    pub amount: Option<u128>,
    pub counterparty: Option<String>,
    pub hash: String,
    pub ok: bool,
}

fn history_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("home dir")
                .join(".alice")
        })
        .join("AliceWallet")
        .join("history.json")
}

pub fn load() -> Vec<TxRecord> {
    let path = history_path();
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn append(record: TxRecord) {
    let mut list = load();
    list.insert(0, record);
    if list.len() > 500 {
        list.truncate(500);
    }
    let path = history_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_vec_pretty(&list) {
        let _ = fs::write(&path, data);
    }
}
