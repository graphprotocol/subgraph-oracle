use async_trait::async_trait;
use common::prelude::*;
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

/// Represents the oracle configuration as stored in the graph-monitoring subgraph.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OracleConfig {
    pub version: String,
    pub ipfs_concurrency: String,
    pub ipfs_timeout: String,
    pub min_signal: String,
    pub period: String,
    pub grace_period: String,
    pub supported_data_source_kinds: String,
    pub network_subgraph_deployment_id: String,
    pub epoch_block_oracle_subgraph_deployment_id: String,
    pub subgraph_availability_manager_contract: String,
    pub oracle_index: String,
}

impl OracleConfig {
    /// Returns a list of field names that differ between two configs.
    pub fn diff(&self, other: &OracleConfig) -> Vec<&'static str> {
        let mut changed = Vec::new();
        if self.version != other.version {
            changed.push("version");
        }
        if self.ipfs_concurrency != other.ipfs_concurrency {
            changed.push("ipfs_concurrency");
        }
        if self.ipfs_timeout != other.ipfs_timeout {
            changed.push("ipfs_timeout");
        }
        if self.min_signal != other.min_signal {
            changed.push("min_signal");
        }
        if self.period != other.period {
            changed.push("period");
        }
        if self.grace_period != other.grace_period {
            changed.push("grace_period");
        }
        if self.supported_data_source_kinds != other.supported_data_source_kinds {
            changed.push("supported_data_source_kinds");
        }
        if self.network_subgraph_deployment_id != other.network_subgraph_deployment_id {
            changed.push("network_subgraph_deployment_id");
        }
        if self.epoch_block_oracle_subgraph_deployment_id
            != other.epoch_block_oracle_subgraph_deployment_id
        {
            changed.push("epoch_block_oracle_subgraph_deployment_id");
        }
        if self.subgraph_availability_manager_contract
            != other.subgraph_availability_manager_contract
        {
            changed.push("subgraph_availability_manager_contract");
        }
        if self.oracle_index != other.oracle_index {
            changed.push("oracle_index");
        }
        changed
    }
}

/// Trait for interacting with the graph-monitoring subgraph.
#[async_trait]
pub trait GraphMonitoringSubgraph {
    /// Fetches the current oracle configuration from the subgraph.
    async fn fetch_oracle_config(&self, oracle_index: u64) -> Result<Option<OracleConfig>, Error>;
}

pub struct GraphMonitoringSubgraphImpl {
    endpoint: String,
    client: Client,
}

impl GraphMonitoringSubgraphImpl {
    pub fn new(endpoint: String) -> Self {
        GraphMonitoringSubgraphImpl {
            endpoint,
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }
}

#[derive(Serialize)]
struct GraphqlRequest {
    query: &'static str,
    variables: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct GraphqlResponse {
    data: Option<ResponseData>,
    errors: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseData {
    global_state: Option<GlobalState>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlobalState {
    active_oracles: Vec<Oracle>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    latest_config: OracleConfig,
}

const ORACLE_CONFIG_QUERY: &str = r#"
    query($oracleIndex: String!) {
        globalState(id: "0") {
            activeOracles(where: { index: $oracleIndex }) {
                latestConfig {
                    version
                    ipfsConcurrency
                    ipfsTimeout
                    minSignal
                    period
                    gracePeriod
                    supportedDataSourceKinds
                    networkSubgraphDeploymentId
                    epochBlockOracleSubgraphDeploymentId
                    subgraphAvailabilityManagerContract
                    oracleIndex
                }
            }
        }
    }
"#;

#[async_trait]
impl GraphMonitoringSubgraph for GraphMonitoringSubgraphImpl {
    async fn fetch_oracle_config(&self, oracle_index: u64) -> Result<Option<OracleConfig>, Error> {
        let mut variables = BTreeMap::new();
        variables.insert("oracleIndex".to_string(), oracle_index.to_string());

        let request = GraphqlRequest {
            query: ORACLE_CONFIG_QUERY,
            variables,
        };

        let response: GraphqlResponse = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        if let Some(errors) = response.errors {
            if !errors.is_empty() {
                return Err(anyhow!(
                    "GraphQL errors: {}",
                    serde_json::to_string(&errors)?
                ));
            }
        }

        Ok(response
            .data
            .and_then(|d| d.global_state)
            .and_then(|gs| gs.active_oracles.into_iter().next())
            .map(|o| o.latest_config))
    }
}
