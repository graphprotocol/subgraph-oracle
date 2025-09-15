use async_trait::async_trait;
use bytes::Bytes;
use common::prelude::*;
use common::prometheus;
use moka::future::Cache;
use reqwest::Client;
use std::time::Duration;
use tiny_cid::Cid;

pub enum IpfsError {
    GatewayTimeout(Cid, Error), // Gateway/Cloudflare timed-out
    ClientTimeout(Cid, Error),  // Client timed-out when requesting the file
    NotFound(Cid, Error),       // Manifest not found
    Other(Error),
}

/// All ipfs interactions required by the oracle.
#[async_trait]
pub trait Ipfs {
    /// Download a file.
    async fn cat(&self, cid: Cid) -> Result<Bytes, IpfsError>;

    /// Invalidate cache of CIDs
    fn invalidate_cache(&self);
}

pub struct IpfsImpl {
    endpoint: String,
    semaphore: tokio::sync::Semaphore,
    client: Client,

    // Cache for CIDs; we invalidate this cache between runs to ensure we're checking
    // IPFS regularly
    cache: moka::future::Cache<Cid, Bytes>,

    // If the request times out, the cid is considered unavailable.
    timeout: Duration,
}

impl IpfsImpl {
    pub fn new(endpoint: String, max_concurrent: usize, timeout: Duration) -> Self {
        IpfsImpl {
            client: Client::new(),
            endpoint,
            semaphore: tokio::sync::Semaphore::new(max_concurrent),
            cache: Cache::new(10000),
            timeout,
        }
    }

    async fn call(&self, cid: Cid) -> Result<reqwest::Response, IpfsError> {
        let _permit = self.semaphore.acquire().await;

        // Using standard IPFS gateway URL format
        let url = format!(
            "{}/ipfs/{}",
            self.endpoint.trim_end_matches('/'),
            cid
        );
        self.client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await
            .map(|res| res.error_for_status())
            .and_then(|x| x)
            .map_err(|e| match e.status().map(|e| e.as_u16()) {
                Some(GATEWAY_TIMEOUT) | Some(CLOUDFLARE_TIMEOUT) => {
                    IpfsError::GatewayTimeout(cid, e.into())
                }
                _ if e.is_timeout() => IpfsError::ClientTimeout(cid, e.into()),
                Some(NOT_FOUND) => IpfsError::NotFound(cid, e.into()),
                _ => IpfsError::Other(e.into()),
            })
    }
}

const CLOUDFLARE_TIMEOUT: u16 = 524;
const GATEWAY_TIMEOUT: u16 = 504;
const NOT_FOUND: u16 = 404;

#[async_trait]
impl Ipfs for IpfsImpl {
    /// Download a file.
    async fn cat(&self, cid: Cid) -> Result<Bytes, IpfsError> {
        if self.cache.contains_key(&cid) {
            METRICS.ipfs_cache_hits.inc();
            let cached_bytes = self.cache.get(&cid).await.unwrap();
            return Result::Ok(cached_bytes);
        }

        async fn call_with_retry(
            ipfs: &IpfsImpl,
            cid: Cid,
            retries: usize,
        ) -> Result<reqwest::Response, IpfsError> {
            let mut last_err = None;
            for _ in 0..=retries {
                match ipfs.call(cid).await {
                    Ok(res) => return Ok(res),
                    Err(e) => {
                        last_err = Some(e);
                    }
                }
            }
            Err(last_err.unwrap())
        }

        let res = call_with_retry(self, cid, 1).await;
        METRICS.ipfs_requests_total.inc();
        let final_bytes = res?.bytes().map_err(|e| IpfsError::Other(e.into())).await?;

        self.cache.insert(cid, final_bytes.clone()).await;
        Result::Ok(final_bytes)
    }

    fn invalidate_cache(&self) {
        self.cache.invalidate_all();
    }
}

struct Metrics {
    ipfs_requests_total: prometheus::IntCounter,
    ipfs_cache_hits: prometheus::IntCounter,
}

lazy_static! {
    static ref METRICS: Metrics = Metrics::new();
}

impl Metrics {
    fn new() -> Self {
        Self {
            ipfs_requests_total: prometheus::register_int_counter!(
                "ipfs_requests_total",
                "Total ipfs requests"
            )
            .unwrap(),
            ipfs_cache_hits: prometheus::register_int_counter!(
                "ipfs_cache_hits",
                "Total ipfs cache hits"
            )
            .unwrap(),
        }
    }
}
