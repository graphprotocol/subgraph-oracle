use async_trait::async_trait;
use common::prelude::*;
use common::prometheus;
use ethers::{
    abi::Address,
    contract::abigen,
    core::types::U256,
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
};
use secp256k1::SecretKey;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

#[async_trait]
pub trait StateManager {
    /// Send a transaction to the contract setting the denied status by deployment id.
    async fn deny_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error>;
}

abigen!(RewardsManagerABI, "src/abi/RewardsManager.abi.json");
abigen!(
    SubgraphAvailabilityManagerABI,
    "src/abi/SubgraphAvailabilityManager.abi.json"
);

pub struct RewardsManagerContract {
    contract: RewardsManagerABI<SignerMiddleware<Provider<Http>, LocalWallet>>,
    logger: Logger,
}

impl RewardsManagerContract {
    pub async fn new(
        signing_key: &SecretKey,
        url: Url,
        rewards_manager_contract: Address,
        logger: Logger,
    ) -> Self {
        let http_client = reqwest::ClientBuilder::new()
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let provider = Provider::new(Http::new_with_client(url, http_client));
        let chain_id = provider.get_chainid().await.unwrap().as_u64();
        let wallet = LocalWallet::from_bytes(signing_key.as_ref())
            .unwrap()
            .with_chain_id(chain_id);
        let provider = Arc::new(SignerMiddleware::new(provider, wallet));
        let contract = RewardsManagerABI::new(rewards_manager_contract, provider.clone());
        Self { contract, logger }
    }
}

pub struct SubgraphAvailabilityManagerContract {
    contract: SubgraphAvailabilityManagerABI<SignerMiddleware<Provider<Http>, LocalWallet>>,
    oracle_index: u64,
    logger: Logger,
}

impl SubgraphAvailabilityManagerContract {
    pub async fn new(
        signing_key: &SecretKey,
        url: Url,
        subgraph_availability_manager_contract: Address,
        oracle_index: u64,
        logger: Logger,
    ) -> Self {
        let http_client = reqwest::ClientBuilder::new()
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let provider = Provider::new(Http::new_with_client(url, http_client));
        let chain_id = provider.get_chainid().await.unwrap().as_u64();
        let wallet = LocalWallet::from_bytes(signing_key.as_ref())
            .unwrap()
            .with_chain_id(chain_id);
        let provider = Arc::new(SignerMiddleware::new(provider, wallet));
        let contract = SubgraphAvailabilityManagerABI::new(
            subgraph_availability_manager_contract,
            provider.clone(),
        );
        Self {
            contract,
            oracle_index,
            logger,
        }
    }
}

#[async_trait]
impl StateManager for RewardsManagerContract {
    async fn deny_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error> {
        // 100 is considered as a good chunk size.
        for chunk in denied_status.chunks(100) {
            let ids: Vec<[u8; 32usize]> = chunk.iter().map(|s| s.0).collect();
            let statuses: Vec<bool> = chunk.iter().map(|s| s.1).collect();
            let num_subgraphs = ids.len() as u64;
            let tx = self
                .contract
                .set_denied_many(ids, statuses);

            if let Err(err) = tx.call().await {
                let message = err.decode_revert::<String>().unwrap_or(err.to_string());
                error!(self.logger, "Transaction failed";
                    "message" => message,
                );
            } else {
                tx.send().await?.await?;
                METRICS.denied_subgraphs_total.inc_by(num_subgraphs);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl StateManager for SubgraphAvailabilityManagerContract {
    async fn deny_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error> {
        // 100 is considered as a good chunk size.
        for chunk in denied_status.chunks(100) {
            let ids: Vec<[u8; 32usize]> = chunk.iter().map(|s| s.0).collect();
            let statuses: Vec<bool> = chunk.iter().map(|s| s.1).collect();
            let num_subgraphs = ids.len() as u64;
            let oracle_index = U256::from(self.oracle_index);
            let tx = self
                .contract
                .vote_many(ids, statuses, oracle_index);

            if let Err(err) = tx.call().await {
                let message = err.decode_revert::<String>().unwrap_or(err.to_string());
                error!(self.logger, "Transaction failed";
                    "message" => message,
                );
            } else {
                tx.send().await?.await?;
                METRICS.denied_subgraphs_total.inc_by(num_subgraphs);
            }
        }

        Ok(())
    }
}

pub struct StateManagerDryRun {
    logger: Logger,
}

impl StateManagerDryRun {
    pub fn new(logger: Logger) -> Self {
        Self { logger }
    }
}

#[async_trait]
impl StateManager for StateManagerDryRun {
    async fn deny_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error> {
        for (id, deny_status) in denied_status {
            info!(self.logger, "Change deny status";
                            "id" => hex::encode(id),
                            "status" => deny_status
            )
        }
        Ok(())
    }
}

struct Metrics {
    denied_subgraphs_total: prometheus::IntCounter,
}

lazy_static! {
    static ref METRICS: Metrics = Metrics::new();
}

impl Metrics {
    fn new() -> Self {
        Self {
            denied_subgraphs_total: prometheus::register_int_counter!(
                "denied_subgraphs_total",
                "Total denied subgraphs"
            )
            .unwrap(),
        }
    }
}
