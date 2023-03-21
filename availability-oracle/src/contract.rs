use async_trait::async_trait;
use common::prelude::*;
use common::prometheus;
use common::web3::contract::Options;

#[async_trait]
pub trait RewardsManager {
    /// Send a transaction to the contract setting the denied status by deployment id.
    async fn set_denied_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error>;
}

type Contracts = common::contracts::Contracts<solidity_bindgen::Web3Context, Web3Provider>;

pub struct RewardsManagerContract {
    contracts: Contracts,
}

impl RewardsManagerContract {
    pub fn new(contracts: Contracts) -> Self {
        Self { contracts }
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
            // To avoid gas estimation errors, we use a high enough gas limit for 100 items
            let options = Options::with(|opt| opt.gas = Some(3_000_000.into()));
            self.contracts
                .rewards_manager()
                .send("setDeniedMany", (ids, statuses), Some(options), Some(4))
                .await?;

            METRICS.denied_subgraphs_total.inc_by(num_subgraphs);
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
