use async_trait::async_trait;
use common::prelude::*;
use common::prometheus;
use secp256k1::key::SecretKey;
use std::sync::Arc;
use std::time::{Duration};
use url::Url;
use ethers:: {
    abi::Address,
    contract::abigen,
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
};

#[async_trait]
pub trait RewardsManager {
    /// Send a transaction to the contract setting the denied status by deployment id.
    async fn set_denied_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error>;
}

abigen!(RewardsManagerABI, "src/abi/RewardsManager.abi.json");

pub struct RewardsManagerContract {
    contract: RewardsManagerABI<SignerMiddleware<Provider<Http>, LocalWallet>>,
    logger: Logger,
}

impl RewardsManagerContract {
    pub async fn new(
        signing_key: &SecretKey, 
        url: Url, 
        rewards_manager_contract: String, 
        logger: Logger
    ) -> Self {
        let http_client = reqwest::ClientBuilder::new()
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let provider = Provider::new(Http::new_with_client(url, http_client));
        let chain_id = provider.get_chainid().await.unwrap().as_u64();
        let wallet =
            LocalWallet::from_bytes(signing_key.as_ref()).unwrap().with_chain_id(chain_id);
        let provider = Arc::new(SignerMiddleware::new(provider, wallet));
        let address: Address = rewards_manager_contract.parse().unwrap();
        let contract = RewardsManagerABI::new(address, provider.clone(),);
        Self { contract, logger }
    }
}

#[async_trait]
impl RewardsManager for RewardsManagerContract {
    async fn set_denied_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error> {
        // Based on this gas profile data for `setDeniedMany`:
        // gas-used,items
        // 4517721,200
        // 2271420,100
        // 474431,20
        // 47642,1
        //
        // 100 is considered as a good chunk size.
        for chunk in denied_status.chunks(100) {
            let ids: Vec<[u8; 32usize]> = chunk.iter().map(|s| s.0).collect();
            let statuses: Vec<bool> = chunk.iter().map(|s| s.1).collect();
            let num_subgraphs = ids.len() as u64;
            let tx = self.contract
                .set_denied_many(ids, statuses)
                // To avoid gas estimation errors, we use a high enough gas limit for 100 items
                .gas(ethers::core::types::U256::from(3_000_000u64));
            
            if let Err(err) = tx.call().await {
                let message = err.decode_revert::<String>().unwrap();   
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

pub struct RewardsManagerDryRun {
    logger: Logger,
}

impl RewardsManagerDryRun {
    pub fn new(logger: Logger) -> Self {
        Self { logger }
    }
}

#[async_trait]
impl RewardsManager for RewardsManagerDryRun {
    async fn set_denied_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error> {
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
