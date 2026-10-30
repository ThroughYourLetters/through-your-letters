use std::time::Duration;

/// Back off quota/network failures instead of reconnecting every second.
pub fn retry_delay(failures: u32, initial_seconds: u64, max_seconds: u64) -> Duration {
    Duration::from_secs(
        initial_seconds
            .saturating_mul(1_u64 << failures.saturating_sub(1).min(63))
            .min(max_seconds),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_retry_rate_is_bounded() {
        let seconds: Vec<_> = (1..=11).map(|n| retry_delay(n, 5, 900).as_secs()).collect();
        assert_eq!(seconds, [5, 10, 20, 40, 80, 160, 320, 640, 900, 900, 900]);
        assert_eq!(retry_delay(u32::MAX, 5, 900), Duration::from_secs(900));
    }
}
