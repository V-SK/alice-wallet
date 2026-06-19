//! Alice Wallet core library — the display-independent half of the wallet.
//!
//! Everything reachable from here is pure logic with NO `eframe`/`egui`
//! dependency: chain RPC (subxt transfer/balance/sync), key derivation and the
//! encrypted keystore, the on-disk config / data-dir layout, the multi-wallet
//! profile store, tx history, the self-updater, the miner/node launch planners,
//! and the child-process supervisors. A second binary (e.g. a CLI wallet) can
//! depend on this crate and link ZERO display libraries.
//!
//! The GUI binary (`src/main.rs` + `app` + `ui`) is the ONLY consumer of
//! `eframe`/`egui`; those modules are NOT part of this library and `eframe` is
//! deliberately unreachable from anything below.
//!
//! Modules keep their original `crate::`-qualified intra-references; because
//! they all moved together into this one crate, those paths still resolve. The
//! GUI bin re-imports these modules at its own crate root (`use gui::{..}`) so
//! the unchanged `crate::chain` / `crate::config` / … paths inside `app`/`ui`
//! continue to resolve there too.

pub mod chain;
pub mod config;
pub mod crypto;
pub mod history;
pub mod i18n;
pub mod miner;
pub mod node;
pub mod supervise;
pub mod update;
pub mod wallet_profiles;
