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
}

pub fn http_provider(url: Url) -> Provider<Http> {
    let http_client = reqwest::ClientBuilder::new()
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    Provider::new(Http::new_with_client(url, http_client))
}

pub fn signer_middleware(
    provider: Provider<Http>,
    signing_key: &SecretKey,
    chain_id: u64,
) -> Arc<SignerMiddleware<Provider<Http>, LocalWallet>> {
    let wallet = LocalWallet::from_bytes(signing_key.as_ref())
        .unwrap()
        .with_chain_id(chain_id);
    Arc::new(SignerMiddleware::new(provider, wallet))
}

impl SubgraphAvailabilityManagerContract {
    pub fn new(
        provider: Provider<Http>,
        chain_id: u64,
        signing_key: &SecretKey,
        subgraph_availability_manager_contract: Address,
        oracle_index: u64,
    ) -> Self {
        let contract = SubgraphAvailabilityManagerABI::new(
            subgraph_availability_manager_contract,
            signer_middleware(provider, signing_key, chain_id),
        );
        Self {
            contract,
            oracle_index,
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
            let tx = self.contract.set_denied_many(ids, statuses);

            // Calculate estimated gas
            let estimated_gas_tx = tx.estimate_gas().await;

            let estimated_gas = match estimated_gas_tx {
                Ok(estimate) => estimate,
                Err(err) => {
                    let message = err.decode_revert::<String>().unwrap_or(err.to_string());
                    error!(self.logger, "Transaction failed";
                        "message" => message,
                    );
                    // Return `Ok()` to avoid double error logging
                    return Ok(());
                }
            };

            // Increase the estimated gas by 20%
            let increased_estimate = estimated_gas * U256::from(120) / U256::from(100);

            // Set a legacy gas price explicitly. Left unset, ethers fills the
            // EIP-1559 fields from its default estimator, which hardcodes a
            // 3 gwei priority fee - roughly 150x the Arbitrum base fee.
            let gas_price = self.contract.client().get_gas_price().await?;
            let gas_price_with_buffer = gas_price * U256::from(120) / U256::from(100);

            tx.gas(increased_estimate)
                .gas_price(gas_price_with_buffer)
                .send()
                .await?
                .await?;
            METRICS.denied_subgraphs_total.inc_by(num_subgraphs);
        }

        Ok(())
    }
}

#[async_trait]
pub trait OracleSigner {
    fn oracle_index(&self) -> u64;
    // Number of subgraphs confirmed on chain, and the error that stopped voting, if any.
    async fn vote_many(&self, denied_status: &[([u8; 32], bool)]) -> (u64, Result<(), Error>);
}

#[async_trait]
impl OracleSigner for SubgraphAvailabilityManagerContract {
    fn oracle_index(&self) -> u64 {
        self.oracle_index
    }

    async fn vote_many(&self, denied_status: &[([u8; 32], bool)]) -> (u64, Result<(), Error>) {
        let mut confirmed = 0;
        // 100 is considered as a good chunk size.
        for chunk in denied_status.chunks(100) {
            if let Err(e) = self.vote_chunk(chunk).await {
                return (confirmed, Err(e));
            }
            confirmed += chunk.len() as u64;
        }
        (confirmed, Ok(()))
    }
}

impl SubgraphAvailabilityManagerContract {
    async fn vote_chunk(&self, chunk: &[([u8; 32], bool)]) -> Result<(), Error> {
        let ids: Vec<[u8; 32usize]> = chunk.iter().map(|s| s.0).collect();
        let statuses: Vec<bool> = chunk.iter().map(|s| s.1).collect();
        let oracle_index = U256::from(self.oracle_index);
        let tx = self.contract.vote_many(ids, statuses, oracle_index);

        // Calculate estimated gas
        let estimated_gas_tx = tx.estimate_gas().await;

        let estimated_gas = match estimated_gas_tx {
            Ok(estimate) => estimate,
            Err(err) => {
                let message = err.decode_revert::<String>().unwrap_or(err.to_string());
                return Err(anyhow!("voteMany reverted: {}", message));
            }
        };

        // Increase the estimated gas by 20%
        let increased_estimate = estimated_gas * U256::from(120) / U256::from(100);

        // Set a legacy gas price explicitly. Left unset, ethers fills the
        // EIP-1559 fields from its default estimator, which hardcodes a
        // 3 gwei priority fee - roughly 150x the Arbitrum base fee.
        let gas_price = self.contract.client().get_gas_price().await?;
        let gas_price_with_buffer = gas_price * U256::from(120) / U256::from(100);

        tx.gas(increased_estimate)
            .gas_price(gas_price_with_buffer)
            .send()
            .await?
            .await?;

        Ok(())
    }
}

/// Submits the same deny status through every signer concurrently. Succeeds if
/// at least one signer succeeds; failures of the others are logged.
pub struct MultiStateManager {
    signers: Vec<Box<dyn OracleSigner + Send + Sync>>,
    logger: Logger,
}

impl MultiStateManager {
    pub fn new(signers: Vec<Box<dyn OracleSigner + Send + Sync>>, logger: Logger) -> Self {
        Self { signers, logger }
    }
}

#[async_trait]
impl StateManager for MultiStateManager {
    async fn deny_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error> {
        let results = futures::future::join_all(
            self.signers
                .iter()
                .map(|signer| signer.vote_many(&denied_status)),
        )
        .await;

        let confirmed = results.iter().map(|(n, _)| *n).max().unwrap_or(0);
        METRICS.denied_subgraphs_total.inc_by(confirmed);

        let mut errors = Vec::new();
        for (signer, (n, result)) in self.signers.iter().zip(results) {
            let index = signer.oracle_index().to_string();
            METRICS
                .signer_votes_total
                .with_label_values(&[&index])
                .inc_by(n);
            if let Err(e) = result {
                METRICS
                    .signer_vote_failures_total
                    .with_label_values(&[&index])
                    .inc();
                errors.push((signer.oracle_index(), e));
            }
        }

        if errors.len() < self.signers.len() {
            for (oracle_index, e) in errors {
                error!(self.logger, "Signer failed to submit deny status";
                    "oracle_index" => oracle_index,
                    "error" => format!("{:#}", e)
                );
            }
            return Ok(());
        }
        if errors.len() == 1 {
            return Err(errors.pop().unwrap().1);
        }
        let causes: Vec<String> = errors
            .iter()
            .map(|(oracle_index, e)| format!("oracle_index {}: {:#}", oracle_index, e))
            .collect();
        Err(anyhow!(
            "all {} signers failed: {}",
            errors.len(),
            causes.join("; ")
        ))
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
    signer_votes_total: prometheus::IntCounterVec,
    signer_vote_failures_total: prometheus::IntCounterVec,
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
            signer_votes_total: prometheus::register_int_counter_vec!(
                "signer_votes_total",
                "Subgraph statuses confirmed on chain per signer",
                &["oracle_index"]
            )
            .unwrap(),
            signer_vote_failures_total: prometheus::register_int_counter_vec!(
                "signer_vote_failures_total",
                "Failed deny status submissions per signer",
                &["oracle_index"]
            )
            .unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockSigner {
        index: u64,
        fail: bool,
        barrier: Option<Arc<tokio::sync::Barrier>>,
        calls: Arc<Mutex<Vec<Vec<([u8; 32], bool)>>>>,
    }

    #[async_trait]
    impl OracleSigner for MockSigner {
        fn oracle_index(&self) -> u64 {
            self.index
        }

        async fn vote_many(&self, denied_status: &[([u8; 32], bool)]) -> (u64, Result<(), Error>) {
            if let Some(barrier) = &self.barrier {
                barrier.wait().await;
            }
            self.calls.lock().unwrap().push(denied_status.to_vec());
            if self.fail {
                return (0, Err(anyhow!("mock signer {} failure", self.index)));
            }
            (denied_status.len() as u64, Ok(()))
        }
    }

    fn signers(
        fail: &[bool],
        barrier: Option<Arc<tokio::sync::Barrier>>,
    ) -> (
        MultiStateManager,
        Vec<Arc<Mutex<Vec<Vec<([u8; 32], bool)>>>>>,
    ) {
        let calls: Vec<_> = fail
            .iter()
            .map(|_| Arc::new(Mutex::new(Vec::new())))
            .collect();
        let signers = fail
            .iter()
            .zip(&calls)
            .enumerate()
            .map(|(index, (fail, calls))| {
                Box::new(MockSigner {
                    index: index as u64,
                    fail: *fail,
                    barrier: barrier.clone(),
                    calls: calls.clone(),
                }) as Box<dyn OracleSigner + Send + Sync>
            })
            .collect();
        (
            MultiStateManager::new(signers, common::logging::create_logger()),
            calls,
        )
    }

    #[tokio::test]
    async fn every_signer_submits_the_same_status() {
        let (multi, calls) = signers(&[false, false], None);
        let status = vec![([1u8; 32], true), ([2u8; 32], false)];

        multi.deny_many(status.clone()).await.unwrap();

        for calls in calls {
            assert_eq!(*calls.lock().unwrap(), vec![status.clone()]);
        }
    }

    #[tokio::test]
    async fn partial_failure_is_ok_and_others_still_vote() {
        let (multi, calls) = signers(&[true, false], None);
        let status = vec![([1u8; 32], true)];
        multi.deny_many(status.clone()).await.unwrap();
        assert_eq!(calls[0].lock().unwrap().len(), 1);
        assert_eq!(*calls[1].lock().unwrap(), vec![status]);
    }

    #[tokio::test]
    async fn single_signer_error_passes_through_unchanged() {
        let (multi, _) = signers(&[true], None);
        let err = multi.deny_many(vec![([1u8; 32], true)]).await.unwrap_err();
        assert_eq!(format!("{:#}", err), "mock signer 0 failure");
    }

    #[tokio::test]
    async fn all_signers_failing_reports_every_cause() {
        let (multi, _) = signers(&[true, true], None);
        let err = multi.deny_many(vec![([1u8; 32], true)]).await.unwrap_err();
        let message = format!("{:#}", err);
        assert!(message.contains("all 2 signers failed"));
        assert!(message.contains("oracle_index 0: mock signer 0 failure"));
        assert!(message.contains("oracle_index 1: mock signer 1 failure"));
    }

    #[tokio::test]
    async fn signers_vote_concurrently() {
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let (multi, _) = signers(&[false, false], Some(barrier));
        tokio::time::timeout(
            Duration::from_secs(1),
            multi.deny_many(vec![([1u8; 32], true)]),
        )
        .await
        .expect("signers ran sequentially")
        .unwrap();
    }
}
