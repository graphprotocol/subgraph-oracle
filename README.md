## Subgraph Oracle
The Subgraph Oracle verifies the availability of the subgraph files and does other validity checks, if a subgraph is found to be invalid it will be denied rewards in the rewards manager contract. Usage:

```
USAGE:
    availability-oracle [FLAGS] [OPTIONS] --ipfs <ipfs> --signing-key <signing-key> --subgraph <subgraph> --url <url>

FLAGS:
        --dry-run    log the results but not send a transaction to the rewards manager
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --data-edge-contract <data-edge-contract>
            The address of the DataEdge contract for posting oracle configuration [env: DATA_EDGE_CONTRACT=]

        --epoch-block-oracle-subgraph <subgraph>
            Graphql endpoint to the epoch block oracle subgraph used for fetching supported networks [env: EPOCH_BLOCK_ORACLE_SUBGRAPH=]

        --grace-period <grace-period>
            Grace period, in seconds from subgraph creation, for which subgraphs will not be checked [env: ORACLE_GRACE_PERIOD=]  [default: 0]

        --graph-monitoring-subgraph <graph-monitoring-subgraph>
            GraphQL endpoint to the graph-monitoring subgraph for checking current oracle config [env: GRAPH_MONITORING_SUBGRAPH=]
        
        --ipfs <ipfs>
            IPFS endpoint with access to the subgraph files [env: ORACLE_IPFS=]

        --ipfs-concurrency <ipfs-concurrency>
            Maximum concurrent calls to IPFS [env: ORACLE_IPFS_CONCURRENCY=]  [default: 100]

        --ipfs-timeout <ipfs-timeout>
            IPFS timeout after which a file will be considered unavailable [env: ORACLE_IPFS_TIMEOUT_SECS=]  [default: 30]

        --metrics-port <metrics-port>
             [env: ORACLE_METRICS_PORT=]  [default: 8090]

        --min-signal <min-signal>
            Minimum signal for a subgraph to be checked [env: ORACLE_MIN_SIGNAL=]  [default: 100]

        --oracle-index <oracle-index>
            Comma separated oracle indexes, to be used when voting on SubgraphAvailabilityManager. Paired by position with --signing-key [env: ORACLE_INDEX=]

        --period <period>
            How often the oracle should check the subgraphs. With the default value of 0, the oracle will run once and terminate [env: ORACLE_PERIOD_SECS=]  [default: 0]
        
        --rewards-manager-contract <rewards-manager-contract>
            The address of the rewards manager contract [env: REWARDS_MANAGER_CONTRACT=]

        --signing-key <signing-key>
            Comma separated secret keys of the oracle signers, paired by position with --oracle-index [env: ORACLE_SIGNING_KEY=]

        --subgraph <subgraph>
            Graphql endpoint to the network subgraph [env: ORACLE_SUBGRAPH=]

        --subgraph-availability-manager-contract <subgraph-availability-manager-contract>
            The address of the subgraph availability manager contract [env: SUBGRAPH_AVAILABILITY_MANAGER_CONTRACT=]

        --supported-data-source-kinds <supported-data-source-kinds>...
            a comma separated list of the supported data source kinds [env: SUPPORTED_DATA_SOURCE_KINDS=]  [default: ethereum,ethereum/contract,file/ipfs,substreams,file/arweave]

        --url <url>
            RPC url for the network [env: RPC_URL=]

```

## Examples

### Example command to testing with a dry run:

```
cargo run -p availability-oracle -- \
    --ipfs https://api.thegraph.com/ipfs \
    --subgraph <network-subgraph-url> \
    --min-signal 10000 \
    --url <url> \
    --dry-run
```

### Example command to run `SubgraphAvailabilityManager` configuration:

```
cargo run -p availability-oracle -- \
    --ipfs https://api.thegraph.com/ipfs \
    --subgraph <network-subgraph-url> \
    --min-signal 10000 \
    --url <url> \
    --subgraph-availability-manager-contract <address> \
    --oracle-index <index> \
    --signing-key <signing-key> \
    --data-edge-contract <data-edge-address> \
    --graph-monitoring-subgraph <graph-monitoring-subgraph-url>
```

### Running with multiple signers

An oracle can vote on `SubgraphAvailabilityManager` with several signers from a single instance. Each signer must be registered in the contract at its own oracle index. Pass the keys and indexes as comma separated lists, paired by position:

```
ORACLE_SIGNING_KEY=<signing-key-a>,<signing-key-b>
ORACLE_INDEX=<index-a>,<index-b>
```

Spaces around commas are ignored. Duplicate keys or indexes are rejected at startup, and `--dry-run` validates them when keys are given.

Every signer posts its own configuration to the DataEdge contract and submits the same votes, concurrently. Configuration errors fail at startup, before any transaction is sent. Runtime failures of a single signer (DataEdge post, reverted vote, RPC, insufficient funds) are logged with its `oracle_index` and do not stop the other signers; a run fails only if every signer fails. `RewardsManager` mode supports a single signer.

Per-signer metrics: `signer_votes_total{oracle_index}` and `signer_vote_failures_total{oracle_index}`. `denied_subgraphs_total` counts each change once, regardless of the number of signers.

### Example command to run `RewardsManager` configuration:

```
cargo run -p availability-oracle -- \
    --ipfs https://api.thegraph.com/ipfs \
    --subgraph <network-subgraph-url> \
    --min-signal 10000 \
    --url <url> \
    --rewards-manager-contract <address> \
    --signing-key <signing-key> \
    --data-edge-contract <data-edge-address> \
    --graph-monitoring-subgraph <graph-monitoring-subgraph-url>
```