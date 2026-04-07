use scale_value::{Primitive, Value, ValueDef};
use std::str::FromStr;
use subxt::error::ExtrinsicError;
use subxt::tx::ValidationResult;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::Keypair as Sr25519Keypair;

pub const TOKEN_DECIMALS: u32 = 12;

pub type Client = OnlineClient<PolkadotConfig>;

pub async fn get_client(url: &str) -> Result<Client, String> {
    OnlineClient::<PolkadotConfig>::from_url(url)
        .await
        .map_err(|e| format!("Failed to connect to node: {:?}", e))
}

pub fn validate_address(address: &str) -> Result<(), String> {
    subxt::utils::AccountId32::from_str(address.trim())
        .map(|_| ())
        .map_err(|_| "Invalid address".into())
}

pub fn parse_token_amount(input: &str, decimals: u32) -> Result<u128, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Amount is required".into());
    }
    if trimmed.starts_with('-') {
        return Err("Amount must be positive".into());
    }

    let normalized = trimmed.replace('_', "");
    let parts: Vec<&str> = normalized.split('.').collect();
    if parts.len() > 2 {
        return Err("Amount has too many decimal points".into());
    }

    let whole = parts[0];
    let fractional = parts.get(1).copied().unwrap_or("");
    if whole.is_empty() && fractional.is_empty() {
        return Err("Amount is required".into());
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fractional.chars().all(|c| c.is_ascii_digit())
    {
        return Err("Amount must be a decimal number".into());
    }
    if fractional.len() > decimals as usize {
        return Err(format!(
            "Amount supports at most {} decimal places",
            decimals
        ));
    }

    let multiplier = 10u128
        .checked_pow(decimals)
        .ok_or_else(|| "Unsupported token decimals".to_string())?;
    let whole_units = if whole.is_empty() {
        0
    } else {
        whole
            .parse::<u128>()
            .map_err(|_| "Amount is too large".to_string())?
    };
    let fractional_units = if fractional.is_empty() {
        0
    } else {
        let padded = format!("{:0<width$}", fractional, width = decimals as usize);
        padded
            .parse::<u128>()
            .map_err(|_| "Amount is too large".to_string())?
    };

    let amount = whole_units
        .checked_mul(multiplier)
        .and_then(|value| value.checked_add(fractional_units))
        .ok_or_else(|| "Amount is too large".to_string())?;

    if amount == 0 {
        return Err("Amount must be greater than zero".into());
    }

    Ok(amount)
}

pub async fn get_block_number(_client: &Client) -> Result<u64, String> {
    Err("use fetch_block_number".into())
}

pub async fn fetch_block_number(rpc_url: &str) -> Result<u64, String> {
    use subxt::rpcs::{LegacyRpcMethods, RpcClient};
    let rpc = RpcClient::from_url(rpc_url)
        .await
        .map_err(|e| e.to_string())?;
    let methods: LegacyRpcMethods<subxt::config::RpcConfigFor<PolkadotConfig>> =
        LegacyRpcMethods::new(rpc);
    let header = methods
        .chain_get_header(None)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no header".to_string())?;
    Ok(header.number as u64)
}

pub async fn get_balance(client: &Client, address: &str) -> Result<u128, String> {
    let account_id =
        subxt::utils::AccountId32::from_str(address.trim()).map_err(|_| "Invalid address")?;

    let storage_query = subxt::storage::dynamic("System", "Account");

    let at_block = client.at_current_block().await.map_err(|e| e.to_string())?;
    let result = at_block
        .storage()
        .try_fetch(storage_query, (Value::from_bytes(&account_id.0),))
        .await
        .map_err(|e| e.to_string())?;

    if let Some(data) = result {
        let value: Value = data.decode().map_err(|e| e.to_string())?;

        if let ValueDef::Composite(c) = value.value {
            let values = match c {
                scale_value::Composite::Named(v) => v,
                scale_value::Composite::Unnamed(_) => return Ok(0),
            };

            for (key, val) in values {
                if key == "data" {
                    if let ValueDef::Composite(datac) = &val.value {
                        let data_values = match datac {
                            scale_value::Composite::Named(v) => v,
                            scale_value::Composite::Unnamed(_) => continue,
                        };

                        for (dk, dv) in data_values {
                            if dk == "free" {
                                if let ValueDef::Primitive(Primitive::U128(b)) = dv.value {
                                    return Ok(b);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(0)
}

#[derive(Debug, Clone)]
pub struct StakeInfo {
    pub stake: u128,
    pub status: String,
}

pub async fn get_stake_status(
    client: &Client,
    pallet: &str,
    storage: &str,
    address: &str,
) -> Result<Option<StakeInfo>, String> {
    let account_id =
        subxt::utils::AccountId32::from_str(address.trim()).map_err(|_| "Invalid address")?;

    let storage_query = subxt::storage::dynamic(pallet, storage);

    let at_block = client.at_current_block().await.map_err(|e| e.to_string())?;
    let result = at_block
        .storage()
        .try_fetch(storage_query, (Value::from_bytes(&account_id.0),))
        .await
        .map_err(|e| e.to_string())?;

    if let Some(data) = result {
        let value: Value = data.decode().map_err(|e| e.to_string())?;
        let mut stake = 0u128;
        let mut status = String::new();

        if let ValueDef::Composite(c) = value.value {
            let values = match c {
                scale_value::Composite::Named(v) => v,
                scale_value::Composite::Unnamed(_) => return Ok(None),
            };

            for (k, val) in values {
                if k == "staked" {
                    if let ValueDef::Primitive(Primitive::U128(s)) = val.value {
                        stake = s;
                    }
                } else if k == "status" {
                    if let ValueDef::Variant(v) = &val.value {
                        status = v.name.clone();
                    }
                }
            }
            return Ok(Some(StakeInfo { stake, status }));
        }
    }

    Ok(None)
}

pub async fn transfer(
    client: &Client,
    pair: Sr25519Keypair,
    dest: &str,
    amount: u128,
) -> Result<String, String> {
    let dest_account_id = subxt::utils::AccountId32::from_str(dest.trim())
        .map_err(|_| "Invalid destination address")?;

    let dest_value = Value::unnamed_variant("Id", vec![Value::from_bytes(&dest_account_id.0)]);
    let tx = subxt::tx::dynamic(
        "Balances",
        "transfer_keep_alive",
        (dest_value, Value::u128(amount)),
    );

    let signer = pair;
    let mut tx_client = client.tx().await.map_err(|e| e.to_string())?;
    let signed = tx_client
        .create_signed(&tx, &signer, Default::default())
        .await
        .map_err(|e| format!("Failed to prepare transfer: {:?}", e))?;
    ensure_valid(&signed.validate().await)?;

    let progress = signed
        .submit_and_watch()
        .await
        .map_err(|e| format!("Failed to submit: {:?}", e))?;
    let events = progress
        .wait_for_finalized_success()
        .await
        .map_err(|e| format!("Transaction failed: {:?}", e))?;

    Ok(format!("{:?}", events.extrinsic_hash()))
}

pub async fn stake(
    client: &Client,
    pair: Sr25519Keypair,
    role: &str,
    amount: u128,
    endpoint: &str,
) -> Result<String, String> {
    let func = if role == "scorer" {
        "stake_as_scorer"
    } else {
        "stake_as_aggregator"
    };
    let tx = subxt::tx::dynamic(
        "ProofOfGradient",
        func,
        (
            Value::u128(amount),
            Value::from_bytes(endpoint.trim().as_bytes()),
        ),
    );

    let signer = pair;
    let mut tx_client = client.tx().await.map_err(|e| e.to_string())?;
    let signed = tx_client
        .create_signed(&tx, &signer, Default::default())
        .await
        .map_err(|e| format!("Failed to prepare stake transaction: {:?}", e))?;
    ensure_valid(&signed.validate().await)?;

    let progress = signed
        .submit_and_watch()
        .await
        .map_err(|e| format!("Failed to submit: {:?}", e))?;
    let events = progress
        .wait_for_finalized_success()
        .await
        .map_err(|e| format!("Transaction failed: {:?}", e))?;

    Ok(format!("{:?}", events.extrinsic_hash()))
}

pub async fn unstake(client: &Client, pair: Sr25519Keypair, role: &str) -> Result<String, String> {
    let func = if role == "scorer" {
        "unstake_scorer"
    } else {
        "unstake_aggregator"
    };
    let tx = subxt::tx::dynamic("ProofOfGradient", func, ());

    let signer = pair;
    let mut tx_client = client.tx().await.map_err(|e| e.to_string())?;
    let signed = tx_client
        .create_signed(&tx, &signer, Default::default())
        .await
        .map_err(|e| format!("Failed to prepare unstake transaction: {:?}", e))?;
    ensure_valid(&signed.validate().await)?;

    let progress = signed
        .submit_and_watch()
        .await
        .map_err(|e| format!("Failed to submit: {:?}", e))?;
    let events = progress
        .wait_for_finalized_success()
        .await
        .map_err(|e| format!("Transaction failed: {:?}", e))?;

    Ok(format!("{:?}", events.extrinsic_hash()))
}

fn ensure_valid(validation: &Result<ValidationResult, ExtrinsicError>) -> Result<(), String> {
    match validation {
        Ok(ValidationResult::Valid(_)) => Ok(()),
        Ok(other) => Err(format!(
            "Transaction rejected during preflight: {:?}",
            other
        )),
        Err(e) => Err(format!("Transaction validation failed: {:?}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_whole_and_fractional_amounts() {
        assert_eq!(
            parse_token_amount("1.25", TOKEN_DECIMALS).unwrap(),
            1_250_000_000_000
        );
        assert_eq!(
            parse_token_amount("0.000000000001", TOKEN_DECIMALS).unwrap(),
            1
        );
    }

    #[test]
    fn rejects_invalid_amounts() {
        assert!(parse_token_amount("", TOKEN_DECIMALS).is_err());
        assert!(parse_token_amount("0", TOKEN_DECIMALS).is_err());
        assert!(parse_token_amount("1.0000000000001", TOKEN_DECIMALS).is_err());
        assert!(parse_token_amount("1e3", TOKEN_DECIMALS).is_err());
    }
}
