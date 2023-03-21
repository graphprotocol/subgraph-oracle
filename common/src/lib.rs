pub mod async_cache;
pub mod contracts;
pub mod logging;
pub mod metrics;
pub mod prelude;

pub use prometheus;
pub use web3;

use prelude::*;
use std::future::Future;
use structopt::StructOpt;

/// Handles common needs for services processes. Namely:
///   * Initialization of loggers
///   * Command-line parsing
///   * Logging startup / shutdown
///
/// The signature Result<Never> means that services are intended
/// to run forever, and that exit means there was some sort of problem
/// that could not be recovered from (usually a configuration error)
pub async fn main<Fut, Args: StructOpt>(run: impl FnOnce(Logger, Args) -> Fut) -> Result<()>
where
    Fut: Future<Output = Result<()>>,
{
    let logger = logging::create_logger();
    let args = Args::from_args();
    info!(logger, "Starting service");
    let result = run(logger.clone(), args).await;

    if let Err(e) = &result {
        error!(
            logger,
            "Process exiting";
            "error" => format!("{:?}", e),
        );
    }
    result
}
