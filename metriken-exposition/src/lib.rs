use metriken::histogram::Snapshot;
use metriken::{AtomicHistogram, Counter, Gauge, Lazy, RwLockHistogram};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use warp::Filter;

pub static DEFAULT_PERCENTILES: &[(&str, f64)] = &[
    ("p25", 25.0),
    ("p50", 50.0),
    ("p75", 75.0),
    ("p90", 90.0),
    ("p99", 99.0),
    ("p999", 99.9),
    ("p9999", 99.99),
];

pub struct HttpServer {
    config: Arc<Config>,
}

pub struct Config {
    address: SocketAddr,
    percentiles: Vec<(String, f64)>,
    prometheus: PrometheusConfig,
}

pub struct PrometheusConfig {
    histograms: bool,
    histogram_grouping_power: u8,
}

type HistogramSnapshots = HashMap<String, metriken::histogram::Snapshot>;

static SNAPSHOTS: Lazy<Arc<RwLock<Snapshots>>> =
    Lazy::new(|| Arc::new(RwLock::new(Snapshots::new())));

pub struct Snapshots {
    timestamp: SystemTime,
    previous: HistogramSnapshots,
    deltas: HistogramSnapshots,
}

impl Default for Snapshots {
    fn default() -> Self {
        Self::new()
    }
}

impl Snapshots {
    pub fn new() -> Self {
        let timestamp = SystemTime::now();

        let mut current = HashMap::new();

        for metric in metriken::metrics().iter() {
            let any = if let Some(any) = metric.as_any() {
                any
            } else {
                continue;
            };

            let key = metric.name().to_string();

            let snapshot = if let Some(histogram) = any.downcast_ref::<metriken::AtomicHistogram>()
            {
                histogram.snapshot()
            } else if let Some(histogram) = any.downcast_ref::<metriken::RwLockHistogram>() {
                histogram.snapshot()
            } else {
                None
            };

            if let Some(snapshot) = snapshot {
                current.insert(key, snapshot);
            }
        }

        let deltas = current.clone();

        Self {
            timestamp,
            previous: current,
            deltas,
        }
    }

    pub fn update(&mut self) {
        self.timestamp = SystemTime::now();

        let mut current = HashMap::new();

        for metric in metriken::metrics().iter() {
            let any = if let Some(any) = metric.as_any() {
                any
            } else {
                continue;
            };

            let key = metric.name().to_string();

            let snapshot = if let Some(histogram) = any.downcast_ref::<metriken::AtomicHistogram>()
            {
                histogram.snapshot()
            } else if let Some(histogram) = any.downcast_ref::<metriken::RwLockHistogram>() {
                histogram.snapshot()
            } else {
                None
            };

            if let Some(snapshot) = snapshot {
                if let Some(previous) = self.previous.get(&key) {
                    self.deltas
                        .insert(key.clone(), snapshot.wrapping_sub(previous).unwrap());
                }

                current.insert(key, snapshot);
            }
        }

        self.previous = current;
    }
}

impl Default for HttpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpServer {
    pub fn new() -> Self {
        Self { config: Config {
                address: "0.0.0.0:4242".parse().unwrap(),
                percentiles: DEFAULT_PERCENTILES.iter().map(|(l, v)| (l.to_string(), *v)).collect(),
                prometheus: PrometheusConfig { histograms: true, histogram_grouping_power: 5 },
            }.into()
        }
    }

    /// HTTP exposition
    pub async fn serve(&self) {
        let http = filters::http(self.config.clone());

        warp::serve(http).run(self.config.address).await;
    }
}


mod filters {
    use super::*;

    fn with_config(
        config: Arc<Config>,
    ) -> impl Filter<Extract = (Arc<Config>,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || config.clone())
    }

    /// The combined set of http endpoint filters
    pub fn http(
        config: Arc<Config>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        prometheus_stats(config.clone())
            .or(human_stats(config.clone()))
            // .or(hardware_info())
    }

    /// GET /metrics
    pub fn prometheus_stats(
        config: Arc<Config>,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::path!("metrics")
            .and(warp::get())
            .and(with_config(config))
            .and_then(handlers::prometheus_stats)
    }

    /// GET /vars
    pub fn human_stats(config: Arc<Config>
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        warp::path!("vars")
            .and(warp::get())
            .and(with_config(config))
            .and_then(handlers::human_stats)
    }
}

mod handlers {
    use super::*;
    use crate::SNAPSHOTS;
    use core::convert::Infallible;
    use std::time::UNIX_EPOCH;

    pub async fn prometheus_stats(config: Arc<Config>) -> Result<impl warp::Reply, Infallible> {
        let mut data = Vec::new();

        let snapshots = SNAPSHOTS.read().await;

        let timestamp = snapshots
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        for metric in &metriken::metrics() {
            let any = match metric.as_any() {
                Some(any) => any,
                None => {
                    continue;
                }
            };

            let name = metric.name();

            if name.starts_with("log_") {
                continue;
            }
            if let Some(counter) = any.downcast_ref::<Counter>() {
                if metric.metadata().is_empty() {
                    data.push(format!(
                        "# TYPE {name}_total counter\n{name}_total {}",
                        counter.value()
                    ));
                } else {
                    data.push(format!(
                        "# TYPE {name} counter\n{} {}",
                        metric.formatted(metriken::Format::Prometheus),
                        counter.value()
                    ));
                }
            } else if let Some(gauge) = any.downcast_ref::<Gauge>() {
                data.push(format!(
                    "# TYPE {name} gauge\n{} {}",
                    metric.formatted(metriken::Format::Prometheus),
                    gauge.value()
                ));
            } else if any.downcast_ref::<AtomicHistogram>().is_some()
                || any.downcast_ref::<RwLockHistogram>().is_some()
            {
                if let Some(delta) = snapshots.deltas.get(metric.name()) {
                    let percentiles: Vec<f64> = config.percentiles.iter().map(|(_, p)| *p).collect();

                    if let Ok(result) = delta.percentiles(&percentiles) {
                        for (percentile, value) in result.iter().map(|(p, b)| (p, b.end())) {
                            data.push(format!(
                                "# TYPE {name} gauge\n{name}{{percentile=\"{:02}\"}} {value} {timestamp}",
                                percentile,
                            ));
                        }
                    }
                }
                if config.prometheus.histograms {
                    if let Some(snapshot) = snapshots.previous.get(metric.name()) {
                        let current = snapshot.config().grouping_power();
                        let target = config.prometheus.histogram_grouping_power;

                        // downsample the snapshot if necessary
                        let downsampled: Option<Snapshot> = if current == target {
                            // the powers matched, we don't need to downsample
                            None
                        } else {
                            Some(snapshot.downsample(target).unwrap())
                        };

                        // reassign to either use the downsampled snapshot or the original
                        let snapshot = if let Some(snapshot) = downsampled.as_ref() {
                            snapshot
                        } else {
                            snapshot
                        };

                        // we need to export a total count (free-running)
                        let mut count = 0;
                        // we also need to export a total sum of all observations
                        // which is also free-running
                        let mut sum = 0;

                        let mut entry = format!("# TYPE {name}_distribution histogram\n");
                        for bucket in snapshot {
                            // add this bucket's sum of observations
                            sum += bucket.count() * bucket.end();

                            // add the count to the aggregate
                            count += bucket.count();

                            entry += &format!(
                                "{name}_distribution_bucket{{le=\"{}\"}} {count} {timestamp}\n",
                                bucket.end()
                            );
                        }

                        entry += &format!(
                            "{name}_distribution_bucket{{le=\"+Inf\"}} {count} {timestamp}\n"
                        );
                        entry += &format!("{name}_distribution_count {count} {timestamp}\n");
                        entry += &format!("{name}_distribution_sum {sum} {timestamp}\n");

                        data.push(entry);
                    }
                }
            }
        }

        data.sort();
        data.dedup();
        let mut content = data.join("\n");
        content += "\n";
        let parts: Vec<&str> = content.split('/').collect();
        Ok(parts.join("_"))
    }

    pub async fn human_stats(config: Arc<Config>) -> Result<impl warp::Reply, Infallible> {
        let mut data = Vec::new();

        let snapshots = SNAPSHOTS.read().await;

        for metric in &metriken::metrics() {
            let any = match metric.as_any() {
                Some(any) => any,
                None => {
                    continue;
                }
            };

            if metric.name().starts_with("log_") {
                continue;
            }

            if let Some(counter) = any.downcast_ref::<Counter>() {
                data.push(format!(
                    "{}: {}",
                    metric.formatted(metriken::Format::Simple),
                    counter.value()
                ));
            } else if let Some(gauge) = any.downcast_ref::<Gauge>() {
                data.push(format!(
                    "{}: {}",
                    metric.formatted(metriken::Format::Simple),
                    gauge.value()
                ));
            } else if any.downcast_ref::<AtomicHistogram>().is_some()
                || any.downcast_ref::<RwLockHistogram>().is_some()
            {
                if let Some(delta) = snapshots.deltas.get(metric.name()) {
                    let percentiles: Vec<f64> = config.percentiles.iter().map(|(_, p)| *p).collect();

                    if let Ok(result) = delta.percentiles(&percentiles) {
                        for (value, label) in result
                            .iter()
                            .map(|(_, b)| b.end())
                            .zip(config.percentiles.iter().map(|(l, _)| l))
                        {
                            data.push(format!(
                                "{}/{}: {}",
                                metric.formatted(metriken::Format::Simple),
                                label,
                                value
                            ));
                        }
                    }
                }
            }
        }

        data.sort();
        let mut content = data.join("\n");
        content += "\n";
        Ok(content)
    }
}
