//! GitHub primary rate-limit handling.
//!
//! GitHub returns `X-RateLimit-Limit`, `X-RateLimit-Remaining`, and
//! `X-RateLimit-Reset` on every authenticated REST/GraphQL response.
//! When `remaining` drops below the warn threshold we slow down; if it
//! reaches zero we sleep until the reset timestamp. The math here is
//! pure so it stays easy to test.

use std::time::Duration;

/// Snapshot of GitHub's rate-limit headers for a single response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitSnapshot {
    pub limit: u32,
    pub remaining: u32,
    /// Unix seconds — when the current window resets.
    pub reset_at: u64,
}

/// Action the caller should take in response to the current rate-limit state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitAction {
    /// Plenty of budget remaining — call again immediately.
    Proceed,
    /// Getting close — back off for `delay` before the next call.
    SlowDown { delay: Duration },
    /// Out of budget — sleep at least `delay` before the next call.
    Exhausted { delay: Duration },
}

const WARN_THRESHOLD: u32 = 100;
const SLOW_DOWN_BASE_MS: u64 = 1_000;
const SLOW_DOWN_CAP_MS: u64 = 30_000;
const RESET_HEADROOM_SECS: u64 = 2;

pub fn parse_snapshot(
    limit: Option<&str>,
    remaining: Option<&str>,
    reset_at: Option<&str>,
) -> Option<RateLimitSnapshot> {
    let limit = limit?.trim().parse::<u32>().ok()?;
    let remaining = remaining?.trim().parse::<u32>().ok()?;
    let reset_at = reset_at?.trim().parse::<u64>().ok()?;
    Some(RateLimitSnapshot {
        limit,
        remaining,
        reset_at,
    })
}

/// Decide what the caller should do given the latest snapshot and the
/// current wall-clock time. `now_unix` is broken out so tests stay
/// deterministic.
pub fn decide_action(snapshot: RateLimitSnapshot, now_unix: u64) -> RateLimitAction {
    if snapshot.remaining == 0 {
        let wait_secs = snapshot
            .reset_at
            .saturating_sub(now_unix)
            .saturating_add(RESET_HEADROOM_SECS);
        return RateLimitAction::Exhausted {
            delay: Duration::from_secs(wait_secs.max(1)),
        };
    }
    if snapshot.remaining < WARN_THRESHOLD {
        return RateLimitAction::SlowDown {
            delay: compute_backoff(snapshot.remaining),
        };
    }
    RateLimitAction::Proceed
}

/// Exponential back-off curve as `remaining` approaches zero.
/// Cap prevents runaway sleeps if `remaining` is reported as 1.
fn compute_backoff(remaining: u32) -> Duration {
    if remaining == 0 {
        return Duration::from_millis(SLOW_DOWN_CAP_MS);
    }
    let factor = (WARN_THRESHOLD / remaining.max(1)) as u64;
    let ms = SLOW_DOWN_BASE_MS
        .saturating_mul(factor)
        .min(SLOW_DOWN_CAP_MS);
    Duration::from_millis(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_complete_header_set() {
        let snapshot = parse_snapshot(Some("5000"), Some("4823"), Some("1747250000")).unwrap();
        assert_eq!(snapshot.limit, 5000);
        assert_eq!(snapshot.remaining, 4823);
        assert_eq!(snapshot.reset_at, 1747250000);
    }

    #[test]
    fn returns_none_when_any_header_is_missing() {
        assert!(parse_snapshot(None, Some("100"), Some("1")).is_none());
        assert!(parse_snapshot(Some("100"), None, Some("1")).is_none());
        assert!(parse_snapshot(Some("100"), Some("1"), None).is_none());
    }

    #[test]
    fn returns_none_when_a_header_is_non_numeric() {
        assert!(parse_snapshot(Some("nope"), Some("1"), Some("1")).is_none());
    }

    #[test]
    fn proceeds_when_remaining_is_above_the_warn_threshold() {
        let snapshot = RateLimitSnapshot {
            limit: 5000,
            remaining: 500,
            reset_at: 100,
        };
        assert_eq!(decide_action(snapshot, 0), RateLimitAction::Proceed);
    }

    #[test]
    fn slows_down_as_remaining_approaches_zero() {
        let snapshot = RateLimitSnapshot {
            limit: 5000,
            remaining: 50,
            reset_at: 100,
        };
        match decide_action(snapshot, 0) {
            RateLimitAction::SlowDown { delay } => {
                assert!(delay >= Duration::from_millis(SLOW_DOWN_BASE_MS));
                assert!(delay <= Duration::from_millis(SLOW_DOWN_CAP_MS));
            }
            other => panic!("expected SlowDown, got {other:?}"),
        }
    }

    #[test]
    fn caps_slow_down_delay_when_remaining_is_one() {
        let snapshot = RateLimitSnapshot {
            limit: 5000,
            remaining: 1,
            reset_at: 100,
        };
        match decide_action(snapshot, 0) {
            RateLimitAction::SlowDown { delay } => {
                assert_eq!(delay, Duration::from_millis(SLOW_DOWN_CAP_MS));
            }
            other => panic!("expected SlowDown, got {other:?}"),
        }
    }

    #[test]
    fn reports_exhausted_with_seconds_until_reset_plus_headroom() {
        let snapshot = RateLimitSnapshot {
            limit: 5000,
            remaining: 0,
            reset_at: 1_000,
        };
        match decide_action(snapshot, 950) {
            RateLimitAction::Exhausted { delay } => {
                assert_eq!(delay, Duration::from_secs(50 + RESET_HEADROOM_SECS));
            }
            other => panic!("expected Exhausted, got {other:?}"),
        }
    }

    #[test]
    fn exhausted_delay_is_at_least_one_second_even_when_reset_is_in_the_past() {
        let snapshot = RateLimitSnapshot {
            limit: 5000,
            remaining: 0,
            reset_at: 100,
        };
        match decide_action(snapshot, 200) {
            RateLimitAction::Exhausted { delay } => {
                assert!(delay >= Duration::from_secs(1));
            }
            other => panic!("expected Exhausted, got {other:?}"),
        }
    }
}
