//! Alice Wallet — headless CLI.
//!
//! A standalone wallet binary for headless Linux servers (no display). It links
//! ZERO GUI code: it depends ONLY on the display-free `gui` LIBRARY core
//! (`gui::chain`, `gui::crypto`, `gui::config`, `gui::wallet_profiles`) and
//! NEVER imports `app`, `ui`, `eframe`, or `egui`. As a result this binary pulls
//! in no windowing / OpenGL / font / image-rasterisation dependencies.
//!
//! Network policy mirrors the GUI's signing path: every RPC connection is to a
//! REMOTE node over `wss://` (default `wss://rpc.aliceprotocol.org`), the
//! embedded node is NEVER spawned, and before any balance read or transfer we
//! confirm the connected node's genesis hash is the pinned Alice mainnet genesis
//! (`gui::chain::ALICE_MAINNET_GENESIS_HASH`) — `submit_transfer` enforces this
//! again internally as defence in depth.
//!
//! Keystore unlock prompts for the password on the controlling terminal with
//! NO ECHO (`rpassword`). No password is ever taken from a flag or hardcoded.

use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use gui::chain::{self, TOKEN_DECIMALS};
use gui::config::DEFAULT_RPC_URL;
use gui::crypto::{self, WalletPayload};

#[derive(Parser)]
#[command(
    name = "alice-wallet-cli",
    version,
    about = "Alice Wallet — headless command-line wallet (no GUI).",
    long_about = "Headless Alice Wallet for servers. Reuses the same keystore created by \
                  the Alice Wallet desktop app and talks to a remote Alice node over wss://. \
                  The embedded local node is never started."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the active wallet's SS58 address (no password needed).
    Address,
    /// Connect to the node and print the wallet's free balance in ALICE.
    Balance {
        /// Remote node URL (must be wss:// for a remote host).
        #[arg(long, default_value = DEFAULT_RPC_URL)]
        rpc: String,
    },
    /// Sign + submit a transfer to a recipient (prompts for the keystore password).
    Send {
        /// Recipient SS58 address.
        #[arg(long)]
        to: String,
        /// Amount in ALICE (decimal, e.g. 1.5).
        #[arg(long)]
        amount: String,
        /// Optional local note shown in the confirmation (not sent on-chain).
        #[arg(long)]
        note: Option<String>,
        /// Remote node URL (must be wss:// for a remote host).
        #[arg(long, default_value = DEFAULT_RPC_URL)]
        rpc: String,
        /// Skip the interactive y/N confirmation (for scripted use).
        #[arg(long)]
        yes: bool,
    },
    /// Sign an arbitrary message (Sign-in-with-Alice); prints the signature hex.
    Sign {
        /// The message to sign.
        #[arg(long)]
        message: String,
    },
    /// Print the active wallet's address as a plain-text "receive" panel.
    Receive,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Address => cmd_address(),
        Command::Balance { rpc } => cmd_balance(&rpc),
        Command::Send {
            to,
            amount,
            note,
            rpc,
            yes,
        } => cmd_send(&to, &amount, note.as_deref(), &rpc, yes),
        Command::Sign { message } => cmd_sign(&message),
        Command::Receive => cmd_receive(),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Load the active wallet's public metadata (no decryption / no password).
fn load_payload() -> Result<WalletPayload, String> {
    crypto::load_active_wallet_payload().map(|(payload, _path)| payload)
}

fn cmd_address() -> Result<(), String> {
    let payload = load_payload()?;
    println!("{}", payload.address);
    Ok(())
}

fn cmd_receive() -> Result<(), String> {
    let payload = load_payload()?;
    let addr = &payload.address;
    let width = addr.len().max(28) + 4;
    let bar = "─".repeat(width);
    println!("┌{bar}┐");
    println!("│{:^width$}│", "Receive ALICE", width = width);
    println!("├{bar}┤");
    println!("│{:^width$}│", "", width = width);
    println!("│{:^width$}│", addr, width = width);
    println!("│{:^width$}│", "", width = width);
    println!("│{:^width$}│", "Send only ALICE to this address.", width = width);
    println!("└{bar}┘");
    Ok(())
}

fn cmd_balance(rpc: &str) -> Result<(), String> {
    let payload = load_payload()?;
    let rt = tokio_runtime()?;
    rt.block_on(async {
        let client = connect_verified(rpc).await?;
        let planck = chain::get_balance(&client, &payload.address).await?;
        println!("Address: {}", payload.address);
        println!("Node:    {rpc}");
        println!("Balance: {} ALICE", format_planck(planck));
        Ok(())
    })
}

fn cmd_send(to: &str, amount: &str, note: Option<&str>, rpc: &str, assume_yes: bool) -> Result<(), String> {
    // Validate inputs BEFORE prompting for the password.
    chain::validate_address(to).map_err(|_| format!("Invalid recipient address: {to}"))?;
    let amount_planck = chain::parse_token_amount(amount, TOKEN_DECIMALS)?;

    let payload = load_payload()?;

    // Show what is about to happen, then require an explicit confirmation.
    println!("About to send:");
    println!("  From:   {}", payload.address);
    println!("  To:     {to}");
    println!("  Amount: {} ALICE", format_planck(amount_planck));
    if let Some(note) = note {
        println!("  Note:   {note}  (local only — not recorded on-chain)");
    }
    println!(
        "  Node:   {rpc}\n  Fee:    paid from sender balance; a small reserve (~{} ALICE) is kept back for fee + keep-alive.",
        format_planck(chain::FEE_ED_MARGIN_PLANCK)
    );

    if !assume_yes && !confirm("Proceed with this transfer? [y/N] ")? {
        return Err("Aborted by user.".into());
    }

    let password = prompt_password("Keystore password: ")?;
    let secrets = crypto::unlock_wallet(&payload, &password)?.secrets;
    let signer = secrets.to_keypair()?;

    let rt = tokio_runtime()?;
    rt.block_on(async {
        // connect_verified + submit_transfer both re-check the genesis hash, so a
        // wrong/stale node can never receive a signed extrinsic.
        let client = connect_verified(rpc).await?;
        let hash = chain::submit_transfer(&client, &signer, to, amount_planck).await?;
        println!("Submitted. Transaction finalized.");
        println!("Tx hash: {hash}");
        Ok(())
    })
}

fn cmd_sign(message: &str) -> Result<(), String> {
    let payload = load_payload()?;
    let password = prompt_password("Keystore password: ")?;
    let secrets = crypto::unlock_wallet(&payload, &password)?.secrets;
    let signer = secrets.to_keypair()?;
    // sr25519 signature, substrate "substrate" signing context — verifiable with
    // subxt_signer::sr25519::verify against the wallet's public key.
    let signature = signer.sign(message.as_bytes());
    println!("Address:   {}", payload.address);
    println!("Public key: {}", payload.public_key);
    println!("Message:   {message}");
    println!("Signature: 0x{}", hex::encode(signature.0));
    Ok(())
}

/// Connect to a node and verify it is Alice mainnet by genesis hash before use.
async fn connect_verified(rpc: &str) -> Result<chain::Client, String> {
    let client = chain::get_client(rpc).await?;
    let genesis = format!("{:?}", client.genesis_hash());
    if genesis != chain::ALICE_MAINNET_GENESIS_HASH {
        return Err(format!(
            "Refusing to trust node {rpc}: genesis {genesis} is not Alice mainnet ({}).",
            chain::ALICE_MAINNET_GENESIS_HASH
        ));
    }
    Ok(client)
}

fn tokio_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Runtime::new().map_err(|e| format!("Failed to start async runtime: {e}"))
}

/// Format an integer planck amount as a human-readable ALICE decimal string,
/// trimming trailing zeros in the fractional part.
fn format_planck(planck: u128) -> String {
    let divisor = 10u128.pow(TOKEN_DECIMALS);
    let whole = planck / divisor;
    let frac = planck % divisor;
    if frac == 0 {
        return whole.to_string();
    }
    let frac_str = format!("{:0width$}", frac, width = TOKEN_DECIMALS as usize);
    let frac_trimmed = frac_str.trim_end_matches('0');
    format!("{whole}.{frac_trimmed}")
}

/// Read a line from stdin and return true only for an explicit yes.
fn confirm(prompt: &str) -> Result<bool, String> {
    print!("{prompt}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read confirmation: {e}"))?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Prompt for a password on the terminal with NO ECHO.
fn prompt_password(prompt: &str) -> Result<String, String> {
    rpassword::prompt_password(prompt)
        .map_err(|e| format!("Failed to read password (no TTY?): {e}"))
}
