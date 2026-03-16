use std::path::{Path, PathBuf};
use std::sync::Arc;

use alloy::consensus::Transaction;
use alloy::network::TransactionResponse;
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};
use chrono::{Duration, Utc};

use crate::db::{RegistrationRepo, SeasonRepo, SqliteRepo};
use crate::domain::Wei;
use crate::services::rcon::RconConfig;
use crate::services::whitelist;

pub fn spawn_payment_poller(
    repo: SqliteRepo,
    rcon_config: Arc<RconConfig>,
    data_dir: PathBuf,
    rpc_url: &str,
    deposit_address: &str,
) -> tokio::task::JoinHandle<()> {
    let rpc_url = rpc_url.to_string();
    let deposit_address = deposit_address.to_string();

    tracing::info!("payment_poller_started");

    tokio::spawn(async move {
        let target: Address = match deposit_address.parse() {
            Ok(addr) => addr,
            Err(e) => {
                tracing::error!(error = %e, "payment_poller_invalid_deposit_address");
                return;
            }
        };

        let url = match rpc_url.parse() {
            Ok(u) => u,
            Err(e) => {
                tracing::error!(error = %e, "payment_poller_invalid_rpc_url");
                return;
            }
        };
        let provider = ProviderBuilder::new().connect_http(url);

        let mut last_block: Option<u64> = None;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;

            let current_block = match provider.get_block_number().await {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "payment_poller_rpc_error");
                    continue;
                }
            };

            let start_block = match last_block {
                Some(lb) => lb + 1,
                None => {
                    tracing::info!(block = current_block, "payment_poller_initial_block");
                    last_block = Some(current_block);
                    continue;
                }
            };

            if start_block > current_block {
                continue;
            }

            let gap = current_block - start_block + 1;
            if gap > 10 {
                tracing::warn!(
                    gap,
                    from = start_block,
                    to = current_block,
                    "payment_poller_block_gap"
                );
            }

            for block_num in start_block..=current_block {
                match provider.get_block_by_number(block_num.into()).full().await {
                    Ok(Some(block)) => {
                        for tx in block.transactions.txns() {
                            if tx.to() == Some(target) {
                                let wei_str = tx.value().to_string();
                                let tx_hash = tx.tx_hash().to_string();

                                match try_confirm_payment(
                                    &repo,
                                    &rcon_config,
                                    &data_dir,
                                    &wei_str,
                                    &tx_hash,
                                )
                                .await
                                {
                                    Ok(true) => {
                                        tracing::info!(
                                            tx_hash = %tx_hash,
                                            amount_wei = %wei_str,
                                            "payment_confirmed_registration"
                                        );
                                    }
                                    Ok(false) => {
                                        tracing::warn!(
                                            tx_hash = %tx_hash,
                                            amount_wei = %wei_str,
                                            "unmatched_payment"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            error = %e,
                                            tx_hash = %tx_hash,
                                            "payment_confirmation_error"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        tracing::warn!(block = block_num, "payment_poller_block_not_found");
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            block = block_num,
                            "payment_poller_block_fetch_error"
                        );
                    }
                }
            }

            last_block = Some(current_block);
        }
    })
}

async fn try_confirm_payment(
    repo: &SqliteRepo,
    rcon_config: &RconConfig,
    data_dir: &Path,
    wei_str: &str,
    tx_hash: &str,
) -> crate::error::AppResult<bool> {
    let amount = Wei::from_decimal(wei_str.to_string())?;

    let reg = match repo.find_pending_by_amount(&amount).await? {
        Some(reg) => reg,
        None => return Ok(false),
    };

    repo.confirm_registration(reg.id, tx_hash).await?;

    let season = repo.get_active_season().await?;

    whitelist::apply_whitelist_for_registration(
        repo,
        rcon_config,
        data_dir,
        reg.factorio_name.as_str(),
        reg.access_tier,
        season.id,
    )
    .await?;

    Ok(true)
}

pub fn spawn_expiry_cleanup(repo: SqliteRepo, expiry_hours: u64) -> tokio::task::JoinHandle<()> {
    tracing::info!(expiry_hours, "expiry_cleanup_started");

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;

            let cutoff = Utc::now() - Duration::hours(expiry_hours as i64);

            match repo.expire_stale_registrations(cutoff).await {
                Ok(count) if count > 0 => {
                    tracing::info!(count, "registrations_expired");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(error = %e, "expiry_cleanup_error");
                }
            }
        }
    })
}
