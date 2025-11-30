use std::time::Duration;

/// Defines the behavior for reconnection attempts.
pub trait ReconnectionPolicy: Send + Sync {
    /// Returns the delay before the next reconnection attempt.
    /// Returns `None` if no more attempts should be made.
    fn next_retry_delay(&self, retry_count: u32, elapsed_milliseconds: u64) -> Option<Duration>;
}

/// A reconnection policy that never retries.
pub struct NoReconnectPolicy;

impl ReconnectionPolicy for NoReconnectPolicy {
    fn next_retry_delay(&self, _retry_count: u32, _elapsed_milliseconds: u64) -> Option<Duration> {
        None
    }
}

/// A reconnection policy that retries with a constant delay.
pub struct ConstantDelayPolicy {
    delay: Duration,
    max_attempts: Option<u32>,
}

impl ConstantDelayPolicy {
    pub fn new(delay: Duration, max_attempts: Option<u32>) -> Self {
        Self { delay, max_attempts }
    }
}

impl ReconnectionPolicy for ConstantDelayPolicy {
    fn next_retry_delay(&self, retry_count: u32, _elapsed_milliseconds: u64) -> Option<Duration> {
        if let Some(max) = self.max_attempts {
            if retry_count >= max {
                return None;
            }
        }
        Some(self.delay)
    }
}

/// A reconnection policy that retries with a linear backoff.
pub struct LinearBackoffPolicy {
    initial_delay: Duration,
    increment: Duration,
    max_delay: Option<Duration>,
    max_attempts: Option<u32>,
}

impl LinearBackoffPolicy {
    pub fn new(initial_delay: Duration, increment: Duration, max_delay: Option<Duration>, max_attempts: Option<u32>) -> Self {
        Self {
            initial_delay,
            increment,
            max_delay,
            max_attempts,
        }
    }
}

impl ReconnectionPolicy for LinearBackoffPolicy {
    fn next_retry_delay(&self, retry_count: u32, _elapsed_milliseconds: u64) -> Option<Duration> {
        if let Some(max) = self.max_attempts {
            if retry_count >= max {
                return None;
            }
        }

        let delay = self.initial_delay + self.increment * retry_count;
        
        if let Some(max_delay) = self.max_delay {
            if delay > max_delay {
                return Some(max_delay);
            }
        }

        Some(delay)
    }
}

/// A reconnection policy that retries with an exponential backoff.
pub struct ExponentialBackoffPolicy {
    initial_delay: Duration,
    factor: f64,
    max_delay: Option<Duration>,
    max_attempts: Option<u32>,
}

impl ExponentialBackoffPolicy {
    pub fn new(initial_delay: Duration, factor: f64, max_delay: Option<Duration>, max_attempts: Option<u32>) -> Self {
        Self {
            initial_delay,
            factor,
            max_delay,
            max_attempts,
        }
    }
}

impl ReconnectionPolicy for ExponentialBackoffPolicy {
    fn next_retry_delay(&self, retry_count: u32, _elapsed_milliseconds: u64) -> Option<Duration> {
        if let Some(max) = self.max_attempts {
            if retry_count >= max {
                return None;
            }
        }

        let delay_secs = self.initial_delay.as_secs_f64() * self.factor.powi(retry_count as i32);
        let delay = Duration::from_secs_f64(delay_secs);

        if let Some(max_delay) = self.max_delay {
            if delay > max_delay {
                return Some(max_delay);
            }
        }

        Some(delay)
    }
}

/// Configuration for reconnection.
#[derive(Clone)]
pub struct ReconnectionConfig {
    pub policy: std::sync::Arc<dyn ReconnectionPolicy>,
}

impl Default for ReconnectionConfig {
    fn default() -> Self {
        Self {
            policy: std::sync::Arc::new(NoReconnectPolicy),
        }
    }
}
