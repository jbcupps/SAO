use std::time::Duration;

const DEFAULT_DB_STARTUP_MAX_WAIT_SECONDS: u64 = 75;
const DEFAULT_INITIAL_RETRY_MILLISECONDS: u64 = 1_000;
const DEFAULT_MAX_RETRY_MILLISECONDS: u64 = 5_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartupRetryConfig {
    pub max_wait: Duration,
    pub initial_retry_delay: Duration,
    pub max_retry_delay: Duration,
}

impl StartupRetryConfig {
    pub fn from_env() -> Self {
        Self {
            max_wait: Duration::from_secs(Self::read_u64(
                "SAO_STARTUP_DB_MAX_WAIT_SECONDS",
                DEFAULT_DB_STARTUP_MAX_WAIT_SECONDS,
            )),
            initial_retry_delay: Duration::from_millis(Self::read_u64(
                "SAO_STARTUP_DB_INITIAL_RETRY_MILLISECONDS",
                DEFAULT_INITIAL_RETRY_MILLISECONDS,
            )),
            max_retry_delay: Duration::from_millis(Self::read_u64(
                "SAO_STARTUP_DB_MAX_RETRY_MILLISECONDS",
                DEFAULT_MAX_RETRY_MILLISECONDS,
            )),
        }
    }

    pub fn retry_delay_for_attempt(&self, attempt: u32) -> Duration {
        let exponential = 2_u128.saturating_pow(attempt.saturating_sub(1));
        let delay_millis = self
            .initial_retry_delay
            .as_millis()
            .saturating_mul(exponential);
        let capped = delay_millis.min(self.max_retry_delay.as_millis());
        let millis = u64::try_from(capped).unwrap_or(u64::MAX);
        Duration::from_millis(millis.max(1))
    }

    fn read_u64(key: &str, default: u64) -> u64 {
        std::env::var(key)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(default)
    }
}

impl Default for StartupRetryConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::StartupRetryConfig;
    use std::time::Duration;

    #[test]
    fn retry_delay_uses_exponential_backoff_with_cap() {
        let config = StartupRetryConfig {
            max_wait: Duration::from_secs(75),
            initial_retry_delay: Duration::from_secs(1),
            max_retry_delay: Duration::from_secs(5),
        };

        assert_eq!(config.retry_delay_for_attempt(1), Duration::from_secs(1));
        assert_eq!(config.retry_delay_for_attempt(2), Duration::from_secs(2));
        assert_eq!(config.retry_delay_for_attempt(3), Duration::from_secs(4));
        assert_eq!(config.retry_delay_for_attempt(4), Duration::from_secs(5));
        assert_eq!(config.retry_delay_for_attempt(8), Duration::from_secs(5));
    }
}
