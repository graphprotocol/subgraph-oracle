use crate::prelude::*;
use prometheus::{core::Collector, gather, Counter, Encoder as _, Registry, TextEncoder};

/// Panics:
///     Any time Prometheus returns an error this panics.
#[derive(Clone)]
pub struct Metrics {
    inner: Registry,
}

impl Metrics {
    pub fn register(&self, collector: &(impl Collector + Clone + 'static)) {
        let collector = Box::new(collector.clone());
        self.inner.register(collector).unwrap();
    }

    pub fn register_counter(&self, name: &str, help: &str) -> Counter {
        let counter = Counter::new(name, help).unwrap();
        self.register(&counter);
        counter
    }
}

/// Serves metrics in Prometheus format on the given port.
pub fn serve(log: Logger, port: u16) {
    use warp::{get, path::end, serve, Filter as _};
    tokio::spawn({
        // Serve metrics at a get request to the root path
        let metrics = get().and(end()).map(move || {
            let mut buffer = Vec::new();
            TextEncoder::new().encode(&gather(), &mut buffer).unwrap();
            String::from_utf8(buffer).unwrap()
        });
        info!(log, "Starting metrics server at port {port}", port = &port);
        serve(metrics).run(([0, 0, 0, 0], port))
    });
}

pub trait WithErrMetric {
    fn with_err_metric(self, metric: &Counter) -> Self;
}

impl<T, E> WithErrMetric for Result<T, E> {
    fn with_err_metric(self, metric: &Counter) -> Self {
        if self.is_err() {
            metric.inc();
        }
        self
    }
}
