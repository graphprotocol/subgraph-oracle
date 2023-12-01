use crate::contract;
use crate::ipfs::*;
use crate::network_subgraph::*;
use crate::util::bytes32_to_cid_v0;
use crate::util::cid_v0_to_bytes32;
use async_trait::async_trait;
use bytes::Bytes;
use common::prelude::*;
use futures::Stream;
use std::sync::Arc;
use std::time::Duration;
use std::{pin::Pin, str::FromStr};
use tiny_cid::Cid;

const ZERO: &str = "QmWtzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
const ONE: &str = "QmWt111111111111111111111111111111111111111111";
const TWO: &str = "QmWt222222222222222222222222222222222222222222";
const THREE: &str = "QmWt333333333333333333333333333333333333333333";
const FOUR: &str = "QmWt444444444444444444444444444444444444444444";
const FIVE: &str = "QmWt555555555555555555555555555555555555555555";
const SIX: &str = "QmWt666666666666666666666666666666666666666666";
const SEVEN: &str = "QmWt777777777777777777777777777777777777777777";
const SUBSTREAM: &str = "QmWt888888888888888888888888888888888888888888";
const FILE_DS: &str = "QmWt999999999999999999999999999999999999999999";

const UNAVAILABLE_LINK: &str = "QmWt3unavzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";

const VALID_WASM: &str = "QmWt3wasmzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
const VALID_ABI: &str = "QmWt3abizzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
const INVALID_ABI: &str = "QmWt3badAbizzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
const VALID_SCHEMA: &str = "QmWt3schemazzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";

// Test the reconcile logic, mocking the data. Test subgraphs:
// - ZERO - remains invalid
// - ONE - remains valid
// - TWO - becomes invalid due to missing manifest
// - THREE - becomes invalid due to missing link `UNAVAILABLE_LINK`
// - FOUR - becomes valid
// - FIVE - becomes invalid due to invalid ABI
// - SIX - becomes invalid due to invalid manifest
// - SEVEN - becomes invalid due to non-mainnet network
// - SUBSTREAM - becomes invalid due to `kind: substream`
// - FILE_DS - remains valid

#[tokio::test]
async fn test_reconcile() {
    crate::reconcile_deny_list(
        &common::logging::create_logger(),
        &MockIpfs,
        &MockRewardsManager,
        Arc::new(MockSubgraph),
        0,
        Duration::default(),
        &vec!["mainnet".into()],
        &vec![
            "ethereum".into(),
            "ethereum/contract".into(),
            "file/ipfs".into(),
            "substreams".into(),
        ],
    )
    .await
    .unwrap()
}

struct MockSubgraph;

impl NetworkSubgraph for MockSubgraph {
    fn deployments_over_threshold(
        self: Arc<Self>,
        _curation_threshold: u64,
        _grace_period: Duration,
    ) -> Pin<Box<dyn Stream<Item = Result<SubgraphDeployment, Error>>>> {
        let new_subgraph = |id, deny| {
            let id = cid_v0_to_bytes32(&Cid::from_str(id).unwrap());
            Ok(SubgraphDeployment {
                id,
                signal_amount: 0,
                deny,
            })
        };
        futures::stream::iter(vec![
            new_subgraph(ZERO, true),
            new_subgraph(ONE, false),
            new_subgraph(TWO, false),
            new_subgraph(THREE, false),
            new_subgraph(FOUR, true),
            new_subgraph(FIVE, false),
            new_subgraph(SIX, false),
            new_subgraph(SEVEN, false),
            new_subgraph(SUBSTREAM, false),
            new_subgraph(FILE_DS, false),
        ])
        .boxed()
    }
}

struct MockIpfs;

#[async_trait]
impl Ipfs for MockIpfs {
    /// Download a file.
    async fn cat(&self, cid: Cid) -> Result<Bytes, IpfsError> {
        let valid_manifest = include_bytes!("test_files/valid.yaml").to_vec().into();
        match cid.to_string().as_str() {
            ZERO => Err(IpfsError::ClientTimeout(cid, Error::msg("zero"))),
            ONE => Ok(valid_manifest),
            TWO => Err(IpfsError::ClientTimeout(cid, Error::msg("two"))),
            THREE => Ok(include_bytes!("test_files/three.yaml").to_vec().into()),
            FOUR => Ok(valid_manifest),
            FIVE => Ok(include_bytes!("test_files/five.yaml").to_vec().into()),
            SIX => Ok("@".as_bytes().into()), // An invalid manifest.
            SEVEN => Ok(include_bytes!("test_files/seven.yaml").to_vec().into()),
            SUBSTREAM => Ok(include_bytes!("test_files/substream.yaml").to_vec().into()),
            FILE_DS => Ok(include_bytes!("test_files/file_ds.yaml").to_vec().into()),

            UNAVAILABLE_LINK => Err(IpfsError::ClientTimeout(cid, Error::msg("unavail"))),

            VALID_WASM => Ok(include_bytes!("test_files/Contract.wasm").to_vec().into()),
            VALID_ABI => Ok(include_bytes!("test_files/Contract.abi").to_vec().into()),
            VALID_SCHEMA => Ok(include_bytes!("test_files/schema.graphql").to_vec().into()),
            INVALID_ABI => Ok(include_bytes!("test_files/BadContract.abi").to_vec().into()),

            _ => unreachable!("unknown cid"),
        }
    }

    fn invalidate_cache(&self) {
        unreachable!("invalidate cache");
    }
}

struct MockRewardsManager;

#[async_trait]
impl contract::RewardsManager for MockRewardsManager {
    async fn set_denied_many(&self, denied_status: Vec<([u8; 32], bool)>) -> Result<(), Error> {
        let denied_status = denied_status
            .into_iter()
            .map(|(id, deny)| {
                let id = bytes32_to_cid_v0(id).to_string();
                (id, deny)
            })
            .collect::<Vec<_>>();

        assert!(denied_status.len() == 6);
        assert_eq!(denied_status[0], (TWO.to_string(), true));
        assert_eq!(denied_status[1], (THREE.to_string(), true));
        assert_eq!(denied_status[2], (FOUR.to_string(), false));
        assert_eq!(denied_status[3], (FIVE.to_string(), true));
        assert_eq!(denied_status[4], (SIX.to_string(), true));
        assert_eq!(denied_status[5], (SEVEN.to_string(), true));

        Ok(())
    }
}
