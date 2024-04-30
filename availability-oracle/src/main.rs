mod contract;
mod ipfs;
mod manifest;
mod network_subgraph;
mod test;
mod util;

use common::prelude::*;
use common::prometheus;
use contract::*;
use ethers::abi::Address;
use ethers::signers::LocalWallet;
use ethers::signers::Signer;
use ipfs::*;
use manifest::{Abi, DataSource, Manifest, Mapping};
use network_subgraph::*;
use secp256k1::SecretKey;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{fmt::Display, str::FromStr};
use structopt::StructOpt;
use tiny_cid::Cid;
use tokio::time::MissedTickBehavior;
use url::Url;
use util::bytes32_to_cid_v0;

fn parse_secs(secs: &str) -> Result<Duration, Error> {
    Ok(Duration::from_secs(u64::from_str(secs)?))
}

#[derive(StructOpt)]
struct Config {
    #[structopt(
        long,
        env = "ORACLE_IPFS",
        help = "IPFS endpoint with access to the subgraph files"
    )]
    ipfs: String,

    #[structopt(
        long,
        env = "ORACLE_SUBGRAPH",
        help = "Graphql endpoint to the network subgraph"
    )]
    subgraph: String,

    #[structopt(
        long,
        env = "ORACLE_PERIOD_SECS",
        default_value = "0",
        parse(try_from_str = parse_secs),
        help = "How often the oracle should check the subgraphs. \
                With the default value of 0, the oracle will run once and terminate"
    )]
    period: Duration,

    #[structopt(
        long,
        env = "ORACLE_MIN_SIGNAL",
        default_value = "100",
        help = "Minimum signal for a subgraph to be checked"
    )]
    min_signal: u64,

    #[structopt(
        long,
        env = "ORACLE_GRACE_PERIOD",
        default_value = "0",
        help = "Grace period, in seconds from subgraph creation, for which subgraphs will not be checked"
    )]
    grace_period: u64,

    #[structopt(
        long,
        env = "ORACLE_IPFS_CONCURRENCY",
        default_value = "100",
        help = "Maximum concurrent calls to IPFS"
    )]
    ipfs_concurrency: usize,

    #[structopt(
        long,
        env = "ORACLE_IPFS_TIMEOUT_SECS",
        default_value = "30",
        parse(try_from_str = parse_secs),
        help = "IPFS timeout after which a file will be considered unavailable"
    )]
    ipfs_timeout: Duration,

    #[structopt(
        long,
        env = "ORACLE_SIGNING_KEY",
        required_unless("dry-run"),
        help = "The secret key of the oracle for signing transactions"
    )]
    signing_key: Option<String>,

    #[structopt(
        long,
        help = "log the results but not send a transaction to the rewards manager"
    )]
    dry_run: bool,

    #[structopt(long, env = "ORACLE_METRICS_PORT", default_value = "8090")]
    metrics_port: u16,

    #[structopt(
        short,
        long,
        default_value = "mainnet",
        value_delimiter = ",",
        env = "SUPPORTED_NETWORKS",
        help = "a comma separated list of the supported network ids"
    )]
    supported_networks: Vec<String>,

    // Note: `ethereum/contract` is a valid alias for `ethereum`
    #[structopt(
        long,
        default_value = "ethereum,ethereum/contract,file/ipfs,substreams,file/arweave",
        value_delimiter = ",",
        env = "SUPPORTED_DATA_SOURCE_KINDS",
        help = "a comma separated list of the supported data source kinds"
    )]
    supported_data_source_kinds: Vec<String>,

    #[structopt(
        long,
        env = "SUBGRAPH_AVAILABILITY_MANAGER_CONTRACT",
        help = "The address of the subgraph availability manager contract"
    )]
    pub subgraph_availability_manager_contract: Option<Address>,

    #[structopt(
        long,
        env = "REWARDS_MANAGER_CONTRACT",
        help = "The address of the rewards manager contract"
    )]
    pub rewards_manager_contract: Option<Address>,

    #[structopt(long, env = "RPC_URL", help = "RPC url for the network")]
    pub url: Url,

    #[structopt(
        long,
        env = "ORACLE_INDEX",
        help = "Assigned index for the oracle, to be used when voting on SubgraphAvailabilityManager"
    )]
    pub oracle_index: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    common::main(run).await
}

async fn run(logger: Logger, config: Config) -> Result<()> {
    let ipfs = IpfsImpl::new(config.ipfs, config.ipfs_concurrency, config.ipfs_timeout);
    let subgraph = NetworkSubgraphImpl::new(logger.clone(), config.subgraph);
    let contract: Box<dyn StateManager> = if config.dry_run {
        Box::new(StateManagerDryRun::new(logger.clone()))
    } else {
        let signing_key: &SecretKey = &config.signing_key.unwrap().parse()?;
        let wallet = LocalWallet::from_bytes(signing_key.as_ref()).unwrap();
        info!(logger, "Signing account {}", wallet.address().to_string());
        state_manager(
            config.url,
            signing_key,
            config.rewards_manager_contract,
            config.subgraph_availability_manager_contract,
            config.oracle_index,
            logger.clone()
        ).await.expect("Configuration error: either [`REWARDS_MANAGER_CONTRACT`] or [`SUBGRAPH_AVAILABILITY_MANAGER_CONTRACT` and `ORACLE_INDEX`] must be provided.")
    };
    let grace_period = Duration::from_secs(config.grace_period);

    common::metrics::serve(logger.clone(), config.metrics_port);

    // Either loop forever or run once and return.
    if config.period > Duration::from_secs(0) {
        let mut interval = tokio::time::interval(config.period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;

            let start = Instant::now();

            METRICS.reconcile_runs_total.inc();

            match reconcile_deny_list(
                &logger,
                &ipfs,
                &*contract,
                subgraph.clone(),
                config.min_signal,
                grace_period,
                &config.supported_networks,
                &config.supported_data_source_kinds,
            )
            .await
            {
                Ok(()) => {
                    METRICS.reconcile_runs_ok.inc();
                }
                Err(e) => {
                    METRICS.reconcile_runs_err.inc();
                    error!(logger, "Error, reconciliation aborted"; "error" => format!("{:#}", e))
                }
            }

            // Log the run time
            info!(
                logger,
                "Processing time: {} ms",
                start.elapsed().as_millis()
            );

            // Invalidate the IPFS cache between runs to ensure that we're checking at least
            // once for every CID per-run
            ipfs.invalidate_cache();
        }
    }
    reconcile_deny_list(
        &logger,
        &ipfs,
        &*contract,
        subgraph,
        config.min_signal,
        grace_period,
        &config.supported_networks,
        &config.supported_data_source_kinds,
    )
    .await
}

// This function is used to create a state manager based on the configuration.
// If subgraph_availability_manager_contract and oracle_index are provided, it will create a SubgraphAvailabilityManagerContract.
// If rewards_manager_contract is provided, it will create a RewardsManagerContract.
// If none of the above are provided, it will return None.
async fn state_manager(
    rpc_url: Url,
    signing_key: &SecretKey,
    rewards_manager_contract: Option<Address>,
    subgraph_availability_manager_contract: Option<Address>,
    oracle_index: Option<u64>,
    logger: Logger,
) -> Option<Box<dyn StateManager>> {
    if let Some(contract_address) = subgraph_availability_manager_contract {
        if let Some(oracle_index) = oracle_index {
            let contract = SubgraphAvailabilityManagerContract::new(
                signing_key,
                rpc_url,
                contract_address,
                oracle_index,
                logger.clone(),
            )
            .await;
            return Some(Box::new(contract));
        }
    } else if let Some(contract_address) = rewards_manager_contract {
        let contract =
            RewardsManagerContract::new(signing_key, rpc_url, contract_address, logger.clone())
                .await;
        return Some(Box::new(contract));
    }

    None
}

/// Does the thing that the availablitiy oracle does, namely:
/// 1. Grab the list of all deployments over the curation threshold from the subgraph.
/// 2. Check if their availability status changed.
/// 3. Update the deny list accordingly.
pub async fn reconcile_deny_list(
    logger: &Logger,
    ipfs: &impl Ipfs,
    state_manager: &dyn contract::StateManager,
    subgraph: Arc<impl NetworkSubgraph>,
    min_signal: u64,
    grace_period: Duration,
    supported_network_ids: &[String],
    supported_ds_kinds: &[String],
) -> Result<(), Error> {
    // Check the availability status of all subgraphs, and gather which should flip the deny flag.
    let status_changes: Vec<([u8; 32], bool)> = subgraph
        .deployments_over_threshold(min_signal, grace_period)
        .map(|deployment| {
            let logger = logger.clone();
            async move {
                let deployment = match deployment {
                    Ok(d) => d,
                    Err(e) => {
                        error!(logger, "Failed to retrieve deployment data"; "error" => e.to_string());
                        return None;
                    }
                };
                let id = bytes32_to_cid_v0(deployment.id);
                let logger = logger.clone();
                match check(ipfs, id, supported_network_ids, supported_ds_kinds).await {
                    Ok(()) => Some((deployment, Valid::Yes)),
                    Err(CheckError::Invalid(e)) => Some((deployment, Valid::No(e))),
                    Err(CheckError::Other(e)) => {
                        error!(logger, "Failed to check subgraph"; "error" => e.to_string());
                        METRICS.reconcile_runs_ipfs_err.inc();
                        None
                    },
                }
            }
        })
        .buffered(100)
        .filter_map(|opt| async move {
            let logger = logger.clone();
            let (deployment, validity) = match opt {
                Some((deployment, validity)) => (deployment, validity),
                None => return None,
            };
            info!(logger, "Check subgraph";
                            "id" => hex::encode(deployment.id),
                            "cid" => deployment.ipfs_hash()
            );
            let should_deny = matches!(validity, Valid::No(_));
            match deployment.deny == should_deny {
                // The validity is unchanged.
                true => {
                    match validity {
                        Valid::Yes => (),
                        // Always print the error reason
                        Valid::No(_) => {
                            info!(logger, "Invalid";
                                "id" => hex::encode(deployment.id),
                                "cid" => deployment.ipfs_hash(),
                                "reason" => validity.to_string(),
                            );
                        }
                    };
                    None
                }

                // The validity status changed, flip the deny flag.
                false => {
                    info!(logger, "Change deny status";
                                    "id" => hex::encode(deployment.id),
                                    "cid" => deployment.ipfs_hash(),
                                    "status" => should_deny,
                                    "reason" => validity.to_string(),
                    );
                    Some((deployment.id, should_deny))
                }
            }
         })
        .collect::<Vec<_>>()
        .await;

    state_manager.deny_many(status_changes).await
}

enum Valid {
    Yes,
    No(Invalid),
}

impl Display for Valid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Valid::Yes => f.write_str("valid"),
            Valid::No(e) => e.fmt(f),
        }
    }
}

const FORBIDDEN_HOST_FN_PREFIX: &[&str; 2] = &["ipfs", "ens"];

enum Invalid {
    BadCid(String),
    Unavailable(Cid, Error),
    ManifestParseError(Error),
    SchemaParseError(Error),
    WasmParseError(Error),
    AbiParseError(Error),
    ForbiddenApi(String),
    UnsupportedNetwork(String),
    UnsupportedDataSourceKind(String),
}

impl Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Invalid::*;

        match self {
            BadCid(cid) => write!(f, "bad cid: {}", cid),
            Unavailable(cid, e) => write!(f, "unavailable cid: {} ({})", cid, e),
            ManifestParseError(e) => write!(f, "manifest parse error: {}", e),
            SchemaParseError(e) => write!(f, "schema parse error: {}", e),
            WasmParseError(e) => write!(f, "wasm parse error: {}", e),
            AbiParseError(e) => write!(f, "abi parse error: {}", e),
            ForbiddenApi(api) => write!(f, "use of forbidden api: {}", api),
            UnsupportedNetwork(network_id) => write!(f, "unsupported network: {}", network_id),
            UnsupportedDataSourceKind(kind) => write!(f, "unsupported data source kind: {}", kind),
        }
    }
}

enum CheckError {
    Invalid(Invalid),
    Other(Error),
}

impl From<IpfsError> for CheckError {
    fn from(e: IpfsError) -> CheckError {
        match e {
            IpfsError::GatewayTimeout(cid, err) => {
                CheckError::Invalid(Invalid::Unavailable(cid, err))
            }
            IpfsError::ClientTimeout(cid, err) => {
                CheckError::Invalid(Invalid::Unavailable(cid, err))
            }
            IpfsError::Other(e) => CheckError::Other(e),
        }
    }
}

impl From<Invalid> for CheckError {
    fn from(e: Invalid) -> CheckError {
        CheckError::Invalid(e)
    }
}

/// Check availability and validity for the manifest and all files linked from it.
/// This requires downloading and parsing the manifest and liked files.
/// An error is a generic networking error from the IPFS request.
async fn check(
    ipfs: &impl Ipfs,
    deployment_id: Cid,
    supported_network_ids: &[String],
    supported_ds_kinds: &[String],
) -> Result<(), CheckError> {
    fn check_link(file: &manifest::Link) -> Result<Cid, Invalid> {
        Cid::from_str(file.link.trim_start_matches("/ipfs/"))
            .map_err(|_| Invalid::BadCid(file.link.to_string()))
    }

    fn calls_any_host_fn<'a>(
        mapping: &'a [u8],
        host_fn_prefixes: &[&str],
    ) -> Result<Option<&'a str>, Error> {
        use wasmparser::Payload;

        for payload in wasmparser::Parser::new(0).parse_all(mapping) {
            if let Payload::ImportSection(s) = payload? {
                for import in s {
                    if let Some(field) = import?.field {
                        if host_fn_prefixes.iter().any(|p| field.starts_with(p)) {
                            return Ok(Some(field));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    // Check if the manifest is available and valid yaml.
    let manifest: Manifest = {
        let raw_manifest = ipfs.cat(deployment_id).await?;
        match serde_yaml::from_slice(&raw_manifest) {
            Ok(manifest) => manifest,
            Err(e) => return Err(Invalid::ManifestParseError(e.into()).into()),
        }
    };

    // Check the schema.
    {
        let schema_cid = check_link(&manifest.schema.file)?;
        let raw_schema = String::from_utf8(ipfs.cat(schema_cid).await?.to_vec())
            .map_err(|e| Invalid::SchemaParseError(e.into()))?;
        graphql_parser::parse_schema::<&str>(&raw_schema)
            .map_err(|e| Invalid::SchemaParseError(e.into()))?;
    }

    let mut network = None;
    for DataSource {
        kind,
        mapping: Mapping { file, abis },
        network: ds_network,
    } in manifest.data_sources()
    {
        // Check data source kind
        if !supported_ds_kinds.contains(kind) {
            return Err(Invalid::UnsupportedDataSourceKind(kind.clone()).into());
        }

        // Check that:
        // - The subgraph has the same network in all data sources.
        // - That network is listed in the `supported_networks` list
        match (network, ds_network) {
            (None, Some(ds_network)) => {
                if !supported_network_ids.contains(ds_network) {
                    return Err(Invalid::UnsupportedNetwork(ds_network.clone()).into());
                }
                network = Some(ds_network)
            }
            (Some(network), Some(ds_network)) => {
                if network != ds_network {
                    return Err(Invalid::ManifestParseError(anyhow!("mismatching networks")).into());
                }
            }
            // Data sources such as file data sources don't have a network
            (_, None) => (),
        }

        // Check that ABIs are valid.
        for Abi { file } in abis {
            ethabi::Contract::load(ipfs.cat(check_link(file)?).await?.as_ref())
                .map_err(|e| Invalid::AbiParseError(e.into()))?;
        }

        // Check mappings.
        if let Some(file) = file {
            let wasm = ipfs.cat(check_link(file)?).await?;
            if let Some(host_fn) = calls_any_host_fn(&wasm, FORBIDDEN_HOST_FN_PREFIX)
                .map_err(Invalid::WasmParseError)?
            {
                return Err(Invalid::ForbiddenApi(host_fn.to_string()).into());
            }
        }
    }

    // All validations have passed.
    Ok(())
}

struct Metrics {
    reconcile_runs_total: prometheus::IntCounter,
    reconcile_runs_ok: prometheus::IntCounter,
    reconcile_runs_err: prometheus::IntCounter,
    reconcile_runs_ipfs_err: prometheus::IntCounter,
}

lazy_static! {
    static ref METRICS: Metrics = Metrics::new();
}

impl Metrics {
    fn new() -> Self {
        Self {
            reconcile_runs_total: prometheus::register_int_counter!(
                "reconcile_runs_total",
                "Total reconcile runs"
            )
            .unwrap(),
            reconcile_runs_ok: prometheus::register_int_counter!(
                "reconcile_runs_ok",
                "Total successful reconcile runs"
            )
            .unwrap(),
            reconcile_runs_err: prometheus::register_int_counter!(
                "reconcile_runs_err",
                "Total reconcile runs with errors"
            )
            .unwrap(),
            reconcile_runs_ipfs_err: prometheus::register_int_counter!(
                "reconcile_runs_ipfs_err",
                "Total reconcile runs with IPFS errors"
            )
            .unwrap(),
        }
    }
}
