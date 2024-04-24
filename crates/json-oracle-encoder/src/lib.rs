use ethabi::{Contract, Token};
use serde::{Deserialize, Serialize};

const ABI_BYTES: &[u8] = include_bytes!("./abi/submitConfigABI.json");

#[derive(Serialize, Deserialize)]
struct Config {
    ipfs_concurrency: String,
    ipfs_timeout: String,
    min_signal: String,
    period: String,
    grace_period: String,
    supported_data_source_kinds: String,
    subgraph: String,
    subgraph_availability_manager_contract: String,
    oracle_index: String,
}

#[derive(Serialize, Deserialize)]
struct Data {
    commit_hash: String,
    config: Config,
}

pub fn json_to_calldata(json: serde_json::Value) -> anyhow::Result<Vec<u8>> {
    let contract = Contract::load(ABI_BYTES)?;
    let function = contract.function("submitConfig")?;

    let data: Data = serde_json::from_value(json)?;

    let tokens = vec![
        Token::String(data.commit_hash),
        Token::Tuple(vec![
            Token::String(data.config.ipfs_concurrency),
            Token::String(data.config.ipfs_timeout),
            Token::String(data.config.min_signal),
            Token::String(data.config.period),
            Token::String(data.config.grace_period),
            Token::String(data.config.supported_data_source_kinds),
            Token::String(data.config.subgraph),
            Token::String(data.config.subgraph_availability_manager_contract),
            Token::String(data.config.oracle_index),
        ]),
    ];

    let encoded_data = function.encode_input(&tokens)?;
    return Ok(encoded_data);
}
