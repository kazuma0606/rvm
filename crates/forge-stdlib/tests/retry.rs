use std::cell::Cell;
use std::thread::sleep;
use std::time::{Duration, Instant};

use forge_stdlib::retry::{
    constant, exponential, linear, retry, retry_with_backoff, with_backoff, CircuitBreaker,
};

#[test]
fn test_retry_succeeds_after_failures() {
    let attempts = Cell::new(0);
    let result = retry(3, || {
        attempts.set(attempts.get() + 1);
        if attempts.get() < 3 {
            Err("failure".to_string())
        } else {
            Ok(attempts.get())
        }
    });
    assert_eq!(result.unwrap(), 3);
}

#[test]
fn test_retry_exhausted_returns_last_error() {
    let attempts = Cell::new(0);
    let result: Result<(), String> = retry(2, || {
        attempts.set(attempts.get() + 1);
        Err(format!("failed {}", attempts.get()))
    });
    assert_eq!(result.unwrap_err(), "failed 2");
}

#[test]
fn test_retry_with_backoff_supports_strategies() {
    let counter = Cell::new(0);
    let backoff = with_backoff(exponential(1), true);
    let result = retry_with_backoff(3, Some(backoff), || {
        counter.set(counter.get() + 1);
        if counter.get() == 3 {
            Ok(())
        } else {
            Err("try again".to_string())
        }
    });
    assert!(result.is_ok());
    assert_eq!(counter.get(), 3);
}

#[test]
fn test_exponential_backoff_timing() {
    // base=10ms, jitter=false → delays: 10ms, 20ms (attempts 1 and 2 fail, attempt 3 succeeds)
    // total expected sleep >= 30ms
    let backoff = with_backoff(exponential(10), false);
    let attempts = Cell::new(0u64);
    let start = Instant::now();
    let result = retry_with_backoff(3, Some(backoff), || {
        attempts.set(attempts.get() + 1);
        if attempts.get() < 3 {
            Err("not yet".to_string())
        } else {
            Ok(())
        }
    });
    let elapsed = start.elapsed();
    assert!(result.is_ok());
    assert!(
        elapsed >= Duration::from_millis(30),
        "expected >= 30ms elapsed, got {:?}",
        elapsed
    );
}

#[test]
fn test_circuit_breaker_opens_after_threshold() {
    let mut breaker = CircuitBreaker::new(2, 100);
    assert!(breaker
        .call(|| Err::<(), String>("bad".to_string()))
        .is_err());
    assert!(breaker
        .call(|| Err::<(), String>("bad".to_string()))
        .is_err());
    assert_eq!(
        breaker.call(|| Ok::<&str, String>("ignored")).unwrap_err(),
        "circuit is open"
    );
}

#[test]
fn test_circuit_breaker_half_open_recovery() {
    let mut breaker = CircuitBreaker::new(2, 10);
    assert!(breaker
        .call(|| Err::<(), String>("bad".to_string()))
        .is_err());
    assert!(breaker
        .call(|| Err::<(), String>("bad".to_string()))
        .is_err());
    sleep(Duration::from_millis(15));
    let success = breaker.call(|| Ok("recovered"));
    assert_eq!(success.unwrap(), "recovered");
    assert!(breaker.call(|| Ok("next")).is_ok());
}
