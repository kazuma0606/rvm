use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

use forge_stdlib::metrics::{timed, InMemoryBackend, LogBackend, MetricsBackend, MetricsLabels};

#[test]
fn test_in_memory_counter_increments() {
    let backend = InMemoryBackend::new();
    backend.counter("requests", 1, None);
    backend.counter("requests", 2, None);
    let counters = backend.counters();
    assert_eq!(counters.get("requests"), Some(&3));
}

#[test]
fn test_in_memory_gauge_updates() {
    let backend = InMemoryBackend::new();
    backend.gauge("load", 0.5, None);
    backend.gauge("load", 0.2, None);
    let gauges = backend.gauges();
    assert_eq!(gauges.get("load"), Some(&0.2));
}

#[test]
fn test_log_backend_outputs_metric() {
    let backend = LogBackend::new();
    let mut labels = MetricsLabels::new();
    labels.insert("service".to_string(), "api".to_string());
    backend.counter("events", 1, Some(&labels));
    let entries = backend.entries();
    assert!(entries.iter().any(|entry| entry.contains("events")));
    assert!(entries
        .iter()
        .any(|entry| entry.contains("service=\"api\"")));
}

#[test]
fn test_timed_records_histogram() {
    let backend = InMemoryBackend::new();
    let result = timed(&backend, "duration", None, || {
        sleep(Duration::from_millis(5));
        Ok("ok")
    });
    assert_eq!(result.unwrap(), "ok");
    let histograms = backend.histograms();
    assert!(histograms.contains_key("duration"));
}
