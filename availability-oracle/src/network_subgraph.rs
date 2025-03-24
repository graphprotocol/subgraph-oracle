use crate::util;
use common::prelude::*;
use futures::stream;
use futures::Stream;
use multibase::Base;
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[derive(Copy, Clone)]
pub struct SubgraphDeployment {
    pub id: [u8; 32],

    // In GRT wei (1/10^18 of a GRT).
    pub signal_amount: u128,
    pub deny: bool,
}

impl SubgraphDeployment {
    pub fn ipfs_hash(&self) -> String {
        let cid = util::bytes32_to_cid_v0(self.id);
        // Unwrap: This is a valid CIDv0 being encoded to Base58
        cid.to_string_of_base(Base::Base58Btc).unwrap()
    }
}

/// Necessary interactions from the network subgraph.
pub trait NetworkSubgraph {
    fn deployments_over_threshold(
        self: Arc<Self>,
        curation_threshold: u64,
        grace_period: Duration,
    ) -> Pin<Box<dyn Stream<Item = Result<SubgraphDeployment, Error>>>>;
}

pub struct NetworkSubgraphImpl {
    logger: Logger,
    endpoint: String,
    client: Client,
}

impl NetworkSubgraphImpl {
    pub fn new(logger: Logger, endpoint: String) -> Arc<Self> {
        Arc::new(NetworkSubgraphImpl {
            logger,
            endpoint,
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap(),
        })
    }
}

#[derive(Serialize)]
struct GraphqlRequest {
    query: String,
    variables: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct GraphqlResponse {
    data: Option<BTreeMap<String, serde_json::Value>>,
    errors: Option<Vec<serde_json::Value>>,
}

const DEPLOYMENTS_QUERY: &str = r#"
query($threshold: BigInt!, $max_creation: Int!, $skip: Int!) {
    subgraphDeployments(first: 1000, skip: $skip, where: { signalledTokens_gt: $threshold, createdAt_lt: $max_creation }) {
        id
        stakedTokens
        deniedAt
    }
}
"#;

impl NetworkSubgraph for NetworkSubgraphImpl {
    // The `curation_threshold` is denominated in GRT.
    fn deployments_over_threshold(
        self: Arc<Self>,
        curation_threshold: u64,
        grace_period: Duration,
    ) -> Pin<Box<dyn Stream<Item = Result<SubgraphDeployment, Error>>>> {
        // Convert the threshold to wei.
        let wei_factor: u128 = 10_u128.pow(18);
        let curation_threshold: u128 = (curation_threshold as u128) * wei_factor;

        let unix_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        let max_creation = (unix_epoch - grace_period).as_secs() as i32;

        stream::iter((0..).step_by(1000))
            .then(move |skip| {
                let this = self.clone();
                async move {
                    let req = GraphqlRequest {
                        query: DEPLOYMENTS_QUERY.to_string(),
                        variables: vec![
                            (
                                "threshold".to_string(),
                                curation_threshold.to_string().into(),
                            ),
                            ("max_creation".to_string(), max_creation.into()),
                            ("skip".to_string(), skip.into()),
                        ]
                        .into_iter()
                        .collect(),
                    };

                    let res: GraphqlResponse = this
                        .client
                        .post(&this.endpoint)
                        .json(&req)
                        .send()
                        .await?
                        .error_for_status()?
                        .json()
                        .await?;

                    if let Some(errs) = res.errors.filter(|errs| !errs.is_empty()) {
                        return Err(anyhow!(
                            "error querying deployments from subgraph {}",
                            serde_json::to_string(&errs)?
                        ));
                    }

                    // Unwrap: A response that has no errors must contain data, the response must
                    // contain a `subgraphDeployments` key.
                    let data = res.data.unwrap().remove("subgraphDeployments").unwrap();

                    #[derive(Deserialize)]
                    #[allow(non_snake_case)]
                    struct RawSubgraphDeployment {
                        id: String,
                        stakedTokens: String,
                        deniedAt: u32,
                    }

                    let page: Vec<RawSubgraphDeployment> = serde_json::from_value(data)?;
                    let page: Vec<SubgraphDeployment> = page
                        .into_iter()
                        .map(|raw_deployment| SubgraphDeployment {
                            // Unwrap: The id returned by the subgraph is a 32 byte long hexadecimal.
                            id: <[u8; 32]>::try_from(
                                hex::decode(raw_deployment.id.trim_start_matches("0x")).unwrap(),
                            )
                            .unwrap(),
                            signal_amount: u128::from_str(&raw_deployment.stakedTokens).unwrap(),
                            deny: raw_deployment.deniedAt > 0,
                        })
                        .collect();

                    trace!(this.logger, "deployments page"; "page_size" => page.len());

                    Ok(page)
                }
            })
            .take_while(|deployments| {
                let keep_paginating = match deployments {
                    Ok(deployments) => !deployments.is_empty(),
                    Err(_) => true,
                };
                async move { keep_paginating }
            })
            .map_ok(|deployments| stream::iter(deployments.into_iter().map(Ok)))
            .try_flatten()
            .boxed()
    }
}
