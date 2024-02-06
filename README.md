## Subgraph Oracle
The Subgraph Oracle verifies the availability of the subgraph files and does other validity checks, if a subgraph is found to be invalid it will be denied rewards in the rewards manager contract. Usage:

```
USAGE:
    availability-oracle [FLAGS] [OPTIONS] --contracts <contracts> --ipfs <ipfs> --signing-key <signing-key> --subgraph <subgraph>

FLAGS:
        --dry-run    log the results but not send a transaction to the rewards manager
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -c, --contracts <contracts>
            One of: `mainnet`, `goerli`, `arbitrum-one`, `arbitrum-goerli`, `ganache/mainnet`, `sepolia` or `arbitrum-sepolia`. See
            `common/src/contracts/config.rs` for the respective configurations [env: ORACLE_CONTRACTS=]
        --grace-period <grace-period>
            Grace period, in seconds from subgraph creation, for which subgraphs will not be checked [env:
            ORACLE_GRACE_PERIOD=]  [default: 0]
        --ipfs <ipfs>
            IPFS endpoint with access to the subgraph files [env: ORACLE_IPFS=]

        --ipfs-concurrency <ipfs-concurrency>
            Maximum concurrent calls to IPFS [env: ORACLE_IPFS_CONCURRENCY=]  [default: 100]

        --ipfs-timeout <ipfs-timeout>
            IPFS timeout after which a file will be considered unavailable [env: ORACLE_IPFS_TIMEOUT_SECS=]  [default:
            30]
        --metrics-port <metrics-port>                    [env: ORACLE_METRICS_PORT=]  [default: 8090]
        --min-signal <min-signal>
            Minimum signal for a subgraph to be checked [env: ORACLE_MIN_SIGNAL=]  [default: 100]

        --period <period>
            How often the oracle should check the subgraphs. With the default value of 0, the oracle will run once and
            terminate [env: ORACLE_PERIOD_SECS=]  [default: 0]
        --signing-key <signing-key>
            The secret key of the oracle for signing transactions [env: ORACLE_SIGNING_KEY=]

        --subgraph <subgraph>                           Graphql endpoint to the network subgraph [env: ORACLE_SUBGRAPH=]
    -s, --supported-networks <supported-networks>...
            a comma separated list of the supported network ids [env: SUPPORTED_NETWORKS=]  [default: mainnet]

```

Example command to testing with a dry run:

```
cargo run -p availability-oracle --  --ipfs https://api.thegraph.com/ipfs  --subgraph https://gateway.thegraph.com/network --dry-run --min-signal 10000
```
