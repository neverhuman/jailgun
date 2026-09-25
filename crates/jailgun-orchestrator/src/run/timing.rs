use std::time::Duration;

use time::OffsetDateTime;

use crate::config::RunOptions;

pub(super) fn run_deadline(opts: &RunOptions, total_tabs: u16) -> Duration {
    let tar_wait_seconds = opts.config.browser.tar_wait_minutes.max(1) as u64 * 60;
    let stagger_seconds = (opts.config.browser.submit_delay_seconds as u64
        + opts.config.browser.submit_jitter_seconds as u64)
        * total_tabs.saturating_sub(1) as u64;
    let derived = Duration::from_secs(tar_wait_seconds + stagger_seconds + 60);
    opts.max_runtime_seconds
        .map(Duration::from_secs)
        .map(|max| derived.min(max))
        .unwrap_or(derived)
}

pub(super) fn submit_delay(opts: &RunOptions) -> Duration {
    let base_ms = opts.config.browser.submit_delay_seconds as u64 * 1_000;
    let jitter_ms = opts.config.browser.submit_jitter_seconds as u64 * 1_000;
    let jitter = if jitter_ms == 0 {
        0
    } else {
        let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos() as u128;
        (nanos % (jitter_ms as u128 + 1)) as u64
    };
    Duration::from_millis(base_ms + jitter)
}
