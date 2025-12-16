use crate::graph_monitoring_subgraph::{GraphMonitoringSubgraph, OracleConfig};
use common::prelude::*;
use ethers::abi::Address;

/// Result of checking config status against the subgraph.
pub enum ConfigStatus {
    /// Config matches what's in the subgraph
    Unchanged,
    /// Config differs from subgraph (includes list of changed field names)
    Changed(Vec<&'static str>),
    /// Oracle not found in subgraph (first time posting)
    NotFound,
    /// Failed to fetch from subgraph
    FetchError(Error),
}
use ethers::core::types::U256;
use ethers::middleware::SignerMiddleware;
use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::TransactionRequest;
use secp256k1::SecretKey;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// Extracts a subgraph deployment ID (CID) from a gateway URL.
/// Expects URLs in the format: https://gateway.thegraph.com/api/[api-key]/deployments/id/Qm...
pub fn extract_deployment_id_from_url(url: &str) -> Result<String, Error> {
    let url = Url::parse(url).map_err(|e| anyhow!("Invalid URL: {}", e))?;

    let path_segments: Vec<&str> = url.path().split('/').collect();
    for (i, segment) in path_segments.iter().enumerate() {
        if *segment == "id" && i + 1 < path_segments.len() {
            let deployment_id = path_segments[i + 1];
            if deployment_id.starts_with("Qm") {
                return Ok(deployment_id.to_string());
            }
        }
    }

    Err(anyhow!(
        "Could not extract deployment ID from URL: {}. Expected format: .../deployments/id/Qm...",
        url
    ))
}

/// Configuration needed to build an OracleConfig from CLI parameters.
pub struct OracleConfigParams<'a> {
    pub ipfs_concurrency: usize,
    pub ipfs_timeout: Duration,
    pub min_signal: u64,
    pub period: Duration,
    pub grace_period: u64,
    pub supported_data_source_kinds: &'a [String],
    pub network_subgraph_url: &'a str,
    pub epoch_block_oracle_subgraph_url: &'a str,
    pub subgraph_availability_manager_contract: Option<Address>,
    pub oracle_index: Option<u64>,
}

/// Builds an OracleConfig from CLI config parameters.
pub fn build_oracle_config(params: &OracleConfigParams) -> Result<OracleConfig, Error> {
    let network_subgraph_deployment_id =
        extract_deployment_id_from_url(params.network_subgraph_url)?;
    let epoch_block_oracle_subgraph_deployment_id =
        extract_deployment_id_from_url(params.epoch_block_oracle_subgraph_url)?;

    Ok(OracleConfig {
        version: format!("v{}", env!("CARGO_PKG_VERSION")),
        ipfs_concurrency: params.ipfs_concurrency.to_string(),
        ipfs_timeout: params.ipfs_timeout.as_millis().to_string(),
        min_signal: params.min_signal.to_string(),
        period: params.period.as_secs().to_string(),
        grace_period: params.grace_period.to_string(),
        supported_data_source_kinds: params.supported_data_source_kinds.join(","),
        network_subgraph_deployment_id,
        epoch_block_oracle_subgraph_deployment_id,
        subgraph_availability_manager_contract: params
            .subgraph_availability_manager_contract
            .map(|a| format!("{:?}", a))
            .unwrap_or_default(),
        oracle_index: params
            .oracle_index
            .map(|i| i.to_string())
            .unwrap_or_default(),
    })
}

/// Checks the local config against the subgraph to determine if it has changed.
pub async fn check_config_status(
    local_config: &OracleConfig,
    monitoring_subgraph: &impl GraphMonitoringSubgraph,
    oracle_index: u64,
) -> ConfigStatus {
    match monitoring_subgraph.fetch_oracle_config(oracle_index).await {
        Ok(Some(current_config)) => {
            if *local_config == current_config {
                ConfigStatus::Unchanged
            } else {
                let changed_fields = local_config.diff(&current_config);
                ConfigStatus::Changed(changed_fields)
            }
        }
        Ok(None) => ConfigStatus::NotFound,
        Err(e) => ConfigStatus::FetchError(e),
    }
}

pub struct DataEdgeContract {
    provider: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    contract_address: Address,
    logger: Logger,
}

impl DataEdgeContract {
    pub async fn new(
        signing_key: &SecretKey,
        rpc_url: Url,
        contract_address: Address,
        logger: Logger,
    ) -> Result<Self, Error> {
        let http_client = reqwest::ClientBuilder::new()
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let provider = Provider::new(Http::new_with_client(rpc_url, http_client));
        let chain_id = provider.get_chainid().await?.as_u64();
        let wallet = LocalWallet::from_bytes(signing_key.as_ref())
            .unwrap()
            .with_chain_id(chain_id);
        let provider = Arc::new(SignerMiddleware::new(provider, wallet));

        Ok(Self {
            provider,
            contract_address,
            logger,
        })
    }

    /// Posts the oracle configuration to the DataEdge contract if it has changed.
    /// Returns Ok(true) if posted, Ok(false) if skipped because unchanged.
    pub async fn post_config_if_changed(
        &self,
        local_config: &OracleConfig,
        monitoring_subgraph: &impl GraphMonitoringSubgraph,
        oracle_index: u64,
    ) -> Result<bool, Error> {
        match check_config_status(local_config, monitoring_subgraph, oracle_index).await {
            ConfigStatus::Unchanged => {
                info!(self.logger, "Config unchanged, skipping DataEdge post";
                    "oracle_index" => oracle_index
                );
                return Ok(false);
            }
            ConfigStatus::Changed(changed_fields) => {
                info!(self.logger, "Config changed, will post to DataEdge";
                    "oracle_index" => oracle_index,
                    "changed_fields" => changed_fields.join(",")
                );
            }
            ConfigStatus::NotFound => {
                info!(self.logger, "Oracle not found in subgraph, posting initial config";
                    "oracle_index" => oracle_index
                );
            }
            ConfigStatus::FetchError(e) => {
                warn!(self.logger, "Failed to fetch current oracle config from subgraph, will post anyway";
                    "oracle_index" => oracle_index,
                    "error" => format!("{:#}", e)
                );
            }
        }

        self.post_config(local_config).await?;
        Ok(true)
    }

    /// Posts the oracle configuration to the DataEdge contract.
    async fn post_config(&self, config: &OracleConfig) -> Result<(), Error> {
        // Build the configuration JSON for posting
        let config_json = serde_json::json!({
            "version": &config.version,
            "config": {
                "ipfs_concurrency": &config.ipfs_concurrency,
                "ipfs_timeout": &config.ipfs_timeout,
                "min_signal": &config.min_signal,
                "period": &config.period,
                "grace_period": &config.grace_period,
                "supported_data_source_kinds": &config.supported_data_source_kinds,
                "network_subgraph_deloyment_id": &config.network_subgraph_deployment_id,
                "epoch_block_oracle_subgraph_deloyment_id": &config.epoch_block_oracle_subgraph_deployment_id,
                "subgraph_availability_manager_contract": &config.subgraph_availability_manager_contract,
                "oracle_index": &config.oracle_index,
            }
        });

        info!(self.logger, "Posting oracle configuration to DataEdge";
            "version" => &config.version,
            "data_edge_contract" => format!("{:?}", self.contract_address),
            "network_subgraph_deployment_id" => &config.network_subgraph_deployment_id,
            "epoch_block_oracle_subgraph_deployment_id" => &config.epoch_block_oracle_subgraph_deployment_id,
        );

        let calldata = json_oracle_encoder::json_to_calldata(config_json)
            .map_err(|e| anyhow!("Failed to encode config as calldata: {}", e))?;

        let gas_price = self.provider.get_gas_price().await?;
        let gas_price_with_buffer = gas_price * U256::from(120) / U256::from(100);

        let tx = TransactionRequest::new()
            .to(self.contract_address)
            .data(calldata.clone());

        let estimated_gas = self.provider.estimate_gas(&tx.clone().into(), None).await?;
        let gas_with_buffer = estimated_gas * U256::from(120) / U256::from(100);

        let tx = tx.gas(gas_with_buffer).gas_price(gas_price_with_buffer);

        let pending_tx = self.provider.send_transaction(tx, None).await?;
        info!(self.logger, "DataEdge transaction sent, waiting for confirmation";
            "tx_hash" => format!("{:?}", pending_tx.tx_hash()),
            "gas_price" => gas_price_with_buffer.as_u64(),
            "gas_limit" => gas_with_buffer.as_u64()
        );

        let receipt = pending_tx
            .await?
            .ok_or_else(|| anyhow!("DataEdge transaction was dropped from mempool"))?;

        info!(self.logger, "Successfully posted config to DataEdge";
            "tx_hash" => format!("{:?}", receipt.transaction_hash),
            "block_number" => receipt.block_number.map(|b| b.as_u64()),
            "gas_used" => receipt.gas_used.map(|g| g.as_u64()),
        );

        Ok(())
    }
}

/// Logs what would happen in dry-run mode by checking against the subgraph.
pub async fn log_dry_run_config(
    logger: &Logger,
    local_config: &OracleConfig,
    monitoring_subgraph: Option<&impl GraphMonitoringSubgraph>,
    oracle_index: Option<u64>,
) {
    if let (Some(subgraph), Some(oracle_index)) = (monitoring_subgraph, oracle_index) {
        match check_config_status(local_config, subgraph, oracle_index).await {
            ConfigStatus::Unchanged => {
                info!(logger, "Config unchanged, would skip DataEdge post (dry-run)";
                    "oracle_index" => oracle_index
                );
            }
            ConfigStatus::Changed(changed_fields) => {
                info!(logger, "Config changed, would post to DataEdge (dry-run)";
                    "oracle_index" => oracle_index,
                    "changed_fields" => changed_fields.join(",")
                );
            }
            ConfigStatus::NotFound => {
                info!(logger, "Oracle not found in subgraph, would post initial config (dry-run)";
                    "oracle_index" => oracle_index
                );
            }
            ConfigStatus::FetchError(e) => {
                warn!(logger, "Failed to fetch current config (dry-run)";
                    "error" => format!("{:#}", e)
                );
            }
        }
    }

    info!(logger, "Local config values";
        "version" => &local_config.version,
        "ipfs_concurrency" => &local_config.ipfs_concurrency,
        "ipfs_timeout" => &local_config.ipfs_timeout,
        "min_signal" => &local_config.min_signal,
        "period" => &local_config.period,
        "grace_period" => &local_config.grace_period,
        "supported_data_source_kinds" => &local_config.supported_data_source_kinds,
        "network_subgraph_deployment_id" => &local_config.network_subgraph_deployment_id,
        "epoch_block_oracle_subgraph_deployment_id" => &local_config.epoch_block_oracle_subgraph_deployment_id,
        "subgraph_availability_manager_contract" => &local_config.subgraph_availability_manager_contract,
        "oracle_index" => &local_config.oracle_index,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    fn test_config() -> OracleConfig {
        OracleConfig {
            version: "v1.0.0".to_string(),
            ipfs_concurrency: "10".to_string(),
            ipfs_timeout: "30000".to_string(),
            min_signal: "100".to_string(),
            period: "60".to_string(),
            grace_period: "10".to_string(),
            supported_data_source_kinds: "ethereum,file/ipfs".to_string(),
            network_subgraph_deployment_id: "Qm123".to_string(),
            epoch_block_oracle_subgraph_deployment_id: "Qm456".to_string(),
            subgraph_availability_manager_contract: "0x123".to_string(),
            oracle_index: "0".to_string(),
        }
    }

    struct MockSubgraphUnchanged(OracleConfig);

    #[async_trait]
    impl GraphMonitoringSubgraph for MockSubgraphUnchanged {
        async fn fetch_oracle_config(
            &self,
            _oracle_index: u64,
        ) -> Result<Option<OracleConfig>, Error> {
            Ok(Some(self.0.clone()))
        }
    }

    struct MockSubgraphChanged(OracleConfig);

    #[async_trait]
    impl GraphMonitoringSubgraph for MockSubgraphChanged {
        async fn fetch_oracle_config(
            &self,
            _oracle_index: u64,
        ) -> Result<Option<OracleConfig>, Error> {
            Ok(Some(self.0.clone()))
        }
    }

    struct MockSubgraphNotFound;

    #[async_trait]
    impl GraphMonitoringSubgraph for MockSubgraphNotFound {
        async fn fetch_oracle_config(
            &self,
            _oracle_index: u64,
        ) -> Result<Option<OracleConfig>, Error> {
            Ok(None)
        }
    }

    struct MockSubgraphError;

    #[async_trait]
    impl GraphMonitoringSubgraph for MockSubgraphError {
        async fn fetch_oracle_config(
            &self,
            _oracle_index: u64,
        ) -> Result<Option<OracleConfig>, Error> {
            Err(anyhow!("Mock fetch error"))
        }
    }

    #[tokio::test]
    async fn test_check_config_status_unchanged() {
        let config = test_config();
        let mock = MockSubgraphUnchanged(config.clone());

        let status = check_config_status(&config, &mock, 0).await;
        assert!(matches!(status, ConfigStatus::Unchanged));
    }

    #[tokio::test]
    async fn test_check_config_status_changed() {
        let local_config = test_config();
        let mut remote_config = test_config();
        remote_config.version = "v2.0.0".to_string();
        remote_config.min_signal = "200".to_string();
        let mock = MockSubgraphChanged(remote_config);

        let status = check_config_status(&local_config, &mock, 0).await;
        match status {
            ConfigStatus::Changed(fields) => {
                assert!(fields.contains(&"version"));
                assert!(fields.contains(&"min_signal"));
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("Expected ConfigStatus::Changed"),
        }
    }

    #[tokio::test]
    async fn test_check_config_status_not_found() {
        let config = test_config();
        let mock = MockSubgraphNotFound;

        let status = check_config_status(&config, &mock, 0).await;
        assert!(matches!(status, ConfigStatus::NotFound));
    }

    #[tokio::test]
    async fn test_check_config_status_fetch_error() {
        let config = test_config();
        let mock = MockSubgraphError;

        let status = check_config_status(&config, &mock, 0).await;
        match status {
            ConfigStatus::FetchError(e) => {
                assert!(e.to_string().contains("Mock fetch error"));
            }
            _ => panic!("Expected ConfigStatus::FetchError"),
        }
    }

    #[test]
    fn test_extract_deployment_id_from_url_valid() {
        // Standard gateway URL format
        let url = "https://gateway.thegraph.com/api/some-api-key/deployments/id/QmSWxvd8SaQK6qZKJ7xtfxCCGoRzGnoi2WNzmJYYJW9BXY";
        assert_eq!(
            extract_deployment_id_from_url(url).unwrap(),
            "QmSWxvd8SaQK6qZKJ7xtfxCCGoRzGnoi2WNzmJYYJW9BXY"
        );

        // Another gateway URL
        let url = "https://gateway-arbitrum.network.thegraph.com/api/key123/deployments/id/QmQEGDTb3xeykCXLdWx7pPX3qeeGMUvHmGWP4SpMkv5QJf";
        assert_eq!(
            extract_deployment_id_from_url(url).unwrap(),
            "QmQEGDTb3xeykCXLdWx7pPX3qeeGMUvHmGWP4SpMkv5QJf"
        );

        // URL with query parameters
        let url = "https://gateway.thegraph.com/api/key/deployments/id/QmSWxvd8SaQK6qZKJ7xtfxCCGoRzGnoi2WNzmJYYJW9BXY?foo=bar";
        assert_eq!(
            extract_deployment_id_from_url(url).unwrap(),
            "QmSWxvd8SaQK6qZKJ7xtfxCCGoRzGnoi2WNzmJYYJW9BXY"
        );
    }

    #[test]
    fn test_extract_deployment_id_from_url_invalid() {
        // Missing /id/ segment
        let url = "https://api.thegraph.com/subgraphs/name/graphprotocol/graph-network-arbitrum";
        assert!(extract_deployment_id_from_url(url).is_err());

        // /id/ segment but no Qm prefix
        let url = "https://gateway.thegraph.com/api/key/deployments/id/not-a-cid";
        assert!(extract_deployment_id_from_url(url).is_err());

        // Invalid URL
        let url = "not-a-valid-url";
        assert!(extract_deployment_id_from_url(url).is_err());

        // Empty URL
        let url = "";
        assert!(extract_deployment_id_from_url(url).is_err());
    }
}
