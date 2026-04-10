use rand::Rng;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Backoff kinds used to space retries.
#[derive(Clone, Copy)]
pub enum BackoffKind {
    Exponential(u64),
    Linear(u64),
    Constant(u64),
}

impl BackoffKind {
    fn base_delay(&self, attempt: u64) -> Duration {
        match self {
            BackoffKind::Constant(ms) => Duration::from_millis(*ms),
            BackoffKind::Linear(step) => Duration::from_millis(step.saturating_mul(attempt)),
            BackoffKind::Exponential(base) => {
                let exp = attempt.saturating_sub(1);
                let multiplier = 1u64.saturating_mul(1u64 << exp.min(63));
                Duration::from_millis(base.saturating_mul(multiplier))
            }
        }
    }
}

/// Configuration returned by `with_backoff`.
pub struct BackoffConfig {
    kind: BackoffKind,
    jitter: bool,
}

impl BackoffConfig {
    fn delay(&self, attempt: u64) -> Duration {
        let base = self.kind.base_delay(attempt);
        if self.jitter {
            let jitter_ms = rand::thread_rng().gen_range(0..=base.as_millis() as u64);
            let jitter = Duration::from_millis(jitter_ms);
            base + jitter
        } else {
            base
        }
    }
}

pub fn exponential(base_ms: u64) -> BackoffKind {
    BackoffKind::Exponential(base_ms)
}

pub fn linear(step_ms: u64) -> BackoffKind {
    BackoffKind::Linear(step_ms)
}

pub fn constant(ms: u64) -> BackoffKind {
    BackoffKind::Constant(ms)
}

pub fn with_backoff(kind: BackoffKind, jitter: bool) -> BackoffConfig {
    BackoffConfig { kind, jitter }
}

fn delay_for_attempt(config: &Option<BackoffConfig>, attempt: u64) {
    if let Some(cfg) = config {
        let duration = cfg.delay(attempt);
        if duration.as_millis() > 0 {
            sleep(duration);
        }
    }
}

pub fn retry<T, F>(max_attempts: u64, operation: F) -> Result<T, String>
where
    F: Fn() -> Result<T, String>,
{
    retry_with_backoff(max_attempts, None, operation)
}

pub fn retry_with_backoff<T, F>(
    max_attempts: u64,
    backoff: Option<BackoffConfig>,
    operation: F,
) -> Result<T, String>
where
    F: Fn() -> Result<T, String>,
{
    if max_attempts == 0 {
        return Err("max_attempts must be greater than 0".to_string());
    }

    let mut attempt = 0;
    loop {
        attempt += 1;
        match operation() {
            Ok(value) => return Ok(value),
            Err(err) => {
                if attempt >= max_attempts {
                    return Err(err);
                }
                delay_for_attempt(&backoff, attempt);
            }
        }
    }
}

#[derive(Clone)]
enum CircuitState {
    Closed,
    Open(Instant),
    HalfOpen,
}

pub struct CircuitBreaker {
    failure_threshold: u64,
    open_timeout: Duration,
    failures: u64,
    state: CircuitState,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u64, open_timeout_ms: u64) -> Self {
        Self {
            failure_threshold: failure_threshold.max(1),
            open_timeout: Duration::from_millis(open_timeout_ms),
            failures: 0,
            state: CircuitState::Closed,
        }
    }

    pub fn call<T, F>(&mut self, operation: F) -> Result<T, String>
    where
        F: Fn() -> Result<T, String>,
    {
        self.update_state();
        match self.state {
            CircuitState::Open(_) => Err("circuit is open".to_string()),
            _ => match operation() {
                Ok(value) => {
                    self.reset();
                    Ok(value)
                }
                Err(err) => {
                    self.on_failure();
                    Err(err)
                }
            },
        }
    }

    fn update_state(&mut self) {
        if let CircuitState::Open(opened) = self.state {
            if opened.elapsed() >= self.open_timeout {
                self.state = CircuitState::HalfOpen;
                self.failures = 0;
            }
        }
    }

    fn on_failure(&mut self) {
        self.failures += 1;
        if self.failures >= self.failure_threshold {
            self.state = CircuitState::Open(Instant::now());
        } else if matches!(self.state, CircuitState::Closed) {
            self.state = CircuitState::Closed;
        } else {
            self.state = CircuitState::HalfOpen;
        }
    }

    fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failures = 0;
    }
}

/// Helper used by decorators to synchronize backend state.
pub type SyncCircuitBreaker = Arc<Mutex<CircuitBreaker>>;
