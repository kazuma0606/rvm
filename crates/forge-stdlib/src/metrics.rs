use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Labels attached to metrics calls.
pub type MetricsLabels = HashMap<String, String>;

pub trait MetricsBackend: Send + Sync {
    fn counter(&self, name: &str, value: i64, labels: Option<&MetricsLabels>);
    fn gauge(&self, name: &str, value: f64, labels: Option<&MetricsLabels>);
    fn histogram(&self, name: &str, value: f64, labels: Option<&MetricsLabels>);
}

fn format_with_labels(name: &str, labels: Option<&MetricsLabels>) -> String {
    if let Some(labels) = labels {
        let mut buf = String::new();
        buf.push_str(name);
        buf.push_str("{");
        let mut first = true;
        for (key, value) in labels {
            if !first {
                buf.push(',');
            }
            first = false;
            write!(buf, "{key}=\"{value}\"", key = key, value = value).ok();
        }
        buf.push('}');
        buf
    } else {
        name.to_string()
    }
}

#[derive(Clone, Default)]
struct MetricsSnapshot {
    counters: HashMap<String, i64>,
    gauges: HashMap<String, f64>,
    histograms: HashMap<String, Vec<f64>>,
}

/// In-memory metrics backend useful for tests and lightweight services.
pub struct InMemoryBackend {
    inner: Arc<Mutex<MetricsSnapshot>>,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MetricsSnapshot::default())),
        }
    }

    pub fn counters(&self) -> HashMap<String, i64> {
        self.inner.lock().unwrap().counters.clone()
    }

    pub fn gauges(&self) -> HashMap<String, f64> {
        self.inner.lock().unwrap().gauges.clone()
    }

    pub fn histograms(&self) -> HashMap<String, Vec<f64>> {
        self.inner.lock().unwrap().histograms.clone()
    }
}

impl MetricsBackend for InMemoryBackend {
    fn counter(&self, name: &str, value: i64, labels: Option<&MetricsLabels>) {
        let key = format_with_labels(name, labels);
        let mut snapshot = self.inner.lock().unwrap();
        let entry = snapshot.counters.entry(key).or_default();
        *entry += value;
    }

    fn gauge(&self, name: &str, value: f64, labels: Option<&MetricsLabels>) {
        let key = format_with_labels(name, labels);
        let mut snapshot = self.inner.lock().unwrap();
        snapshot.gauges.insert(key, value);
    }

    fn histogram(&self, name: &str, value: f64, labels: Option<&MetricsLabels>) {
        let key = format_with_labels(name, labels);
        let mut snapshot = self.inner.lock().unwrap();
        snapshot.histograms.entry(key).or_default().push(value);
    }
}

/// Logger backend that persists records for inspection.
pub struct LogBackend {
    entries: Arc<Mutex<Vec<String>>>,
}

impl LogBackend {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn entries(&self) -> Vec<String> {
        self.entries.lock().unwrap().clone()
    }

    fn record(&self, kind: &str, text: String) {
        let mut guard = self.entries.lock().unwrap();
        guard.push(format!("[{}] {}", kind, text));
    }
}

impl MetricsBackend for LogBackend {
    fn counter(&self, name: &str, value: i64, labels: Option<&MetricsLabels>) {
        let text = format!("counter {} = {}", format_with_labels(name, labels), value);
        self.record("counter", text);
    }

    fn gauge(&self, name: &str, value: f64, labels: Option<&MetricsLabels>) {
        let text = format!("gauge {} = {}", format_with_labels(name, labels), value);
        self.record("gauge", text);
    }

    fn histogram(&self, name: &str, value: f64, labels: Option<&MetricsLabels>) {
        let text = format!("histogram {} = {}", format_with_labels(name, labels), value);
        self.record("histogram", text);
    }
}

/// Prometheus-like emitter that serializes metric calls.
pub struct PrometheusBackend {
    emitted: Arc<Mutex<Vec<String>>>,
}

impl PrometheusBackend {
    pub fn new() -> Self {
        Self {
            emitted: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.emitted.lock().unwrap().clone()
    }
}

impl MetricsBackend for PrometheusBackend {
    fn counter(&self, name: &str, value: i64, labels: Option<&MetricsLabels>) {
        let mut buf = format!("{}{} {}", name, format_suffix(labels), value);
        self.emitted.lock().unwrap().push(buf);
    }

    fn gauge(&self, name: &str, value: f64, labels: Option<&MetricsLabels>) {
        let mut buf = format!("{}{} {}", name, format_suffix(labels), value);
        self.emitted.lock().unwrap().push(buf);
    }

    fn histogram(&self, name: &str, value: f64, labels: Option<&MetricsLabels>) {
        let mut buf = format!("{}{} {}", name, format_suffix(labels), value);
        self.emitted.lock().unwrap().push(buf);
    }
}

/// Statsd-like emitter capturing strings.
pub struct StatsdBackend {
    emitted: Arc<Mutex<Vec<String>>>,
}

impl StatsdBackend {
    pub fn new() -> Self {
        Self {
            emitted: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.emitted.lock().unwrap().clone()
    }
}

impl MetricsBackend for StatsdBackend {
    fn counter(&self, name: &str, value: i64, labels: Option<&MetricsLabels>) {
        let record = format!("{}:{}|c{}", name, value, format_suffix(labels));
        self.emitted.lock().unwrap().push(record);
    }

    fn gauge(&self, name: &str, value: f64, labels: Option<&MetricsLabels>) {
        let record = format!("{}:{}|g{}", name, value, format_suffix(labels));
        self.emitted.lock().unwrap().push(record);
    }

    fn histogram(&self, name: &str, value: f64, labels: Option<&MetricsLabels>) {
        let record = format!("{}:{}|h{}", name, value, format_suffix(labels));
        self.emitted.lock().unwrap().push(record);
    }
}

fn format_suffix(labels: Option<&MetricsLabels>) -> String {
    if let Some(labels) = labels {
        let mut buf = String::new();
        buf.push('{');
        let mut first = true;
        for (key, value) in labels {
            if !first {
                buf.push(',');
            }
            first = false;
            write!(buf, "{key}={value}", key = key, value = value).ok();
        }
        buf.push('}');
        buf
    } else {
        String::new()
    }
}

/// Measures execution time and records it through the backend histogram.
pub fn timed<T, F>(
    backend: &dyn MetricsBackend,
    metric: impl AsRef<str>,
    labels: Option<&MetricsLabels>,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    let start = Instant::now();
    let result = operation();
    let duration = Instant::now().duration_since(start);
    backend.histogram(metric.as_ref(), duration.as_secs_f64(), labels);
    result
}
