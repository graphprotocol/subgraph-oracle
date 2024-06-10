use common::prelude::*;
use futures::stream;
use futures::Stream;
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;

pub trait EpochBlockOracleSubgraph {
    fn supported_networks(self: Arc<Self>) -> Pin<Box<dyn Stream<Item = Result<String, Error>>>>;
}

pub struct EpochBlockOracleSubgraphImpl {
    logger: Logger,
    endpoint: String,
    client: Client,
}

impl EpochBlockOracleSubgraphImpl {
    pub fn new(logger: Logger, endpoint: String) -> Arc<Self> {
        Arc::new(EpochBlockOracleSubgraphImpl {
            logger,
            endpoint,
            client: Client::new(),
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

const SUPPORTED_NETWORKS_QUERY: &str = r#"
query Networks($skip: Int!) {
    globalState(id: "0") {
        networks(first: 1000, skip: $skip) {
            id
            alias
        }
    }
}
"#;

impl EpochBlockOracleSubgraph for EpochBlockOracleSubgraphImpl {
    fn supported_networks(self: Arc<Self>) -> Pin<Box<dyn Stream<Item = Result<String, Error>>>> {
        stream::iter((0..).step_by(1000))
            .then(move |skip| {
                let this = self.clone();
                async move {
                    let req = GraphqlRequest {
                        query: SUPPORTED_NETWORKS_QUERY.to_string(),
                        variables: vec![("skip".to_string(), skip.into())]
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
                            "error querying supported networks from subgraph {}",
                            serde_json::to_string(&errs)?
                        ));
                    }

                    let data = res
                        .data
                        .ok_or_else(|| anyhow!("Data field is missing in the response"))?
                        .remove("globalState")
                        .and_then(|global_state| global_state.get("networks").cloned())
                        .ok_or_else(|| anyhow!("'networks' field is missing in the globalState data"))?;

                    #[derive(Deserialize)]
                    #[allow(non_snake_case)]
                    struct RawNetwork {
                        id: String,
                        alias: String,
                    }

                    let page: Vec<RawNetwork> = serde_json::from_value(data)?;
                    let page: Vec<String> = page
                        .into_iter()
                        .flat_map(|raw_network| vec![raw_network.id, raw_network.alias])
                        .collect();

                    trace!(this.logger, "networks page"; "page_size" => page.len());

                    Ok(page)
                }
            })
            .take_while(|networks| {
                let keep_paginating = match networks {
                    Ok(networks) => !networks.is_empty(),
                    Err(_) => true,
                };
                async move { keep_paginating }
            })
            .map_ok(|networks| stream::iter(networks.into_iter().map(Ok)))
            .try_flatten()
            .boxed()
    }
}
