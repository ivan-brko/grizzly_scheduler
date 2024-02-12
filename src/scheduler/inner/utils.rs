use crate::error::SchedulerError;
use chrono::{DateTime, Duration};
use cron::Schedule;
use rand::Rng;
use std::str::FromStr;
use tracing::warn;
use uuid::Uuid;

pub(super) fn parse_cron(cron_string: &str) -> Result<Schedule, SchedulerError> {
    Schedule::from_str(cron_string).map_err(|e| {
        warn!("Grizzly Scheduler: Invalid cron expression: {}", e);
        SchedulerError::CronParseError(e.to_string())
    })
}

pub(super) fn generate_job_id() -> String {
    format!(
        "grizzly_job/{}",
        Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>()
    )
}

#[inline]
pub(super) fn generate_single_run_id() -> String {
    format!("grizzly_single_run/{}", Uuid::new_v4())
}

#[inline]
pub(super) fn maybe_with_fuzzy_offset(
    offset_without_fuzzy: Duration,
    fuzzy_effect: Option<Duration>,
) -> Duration {
    if let Some(fuzzy_effect) = fuzzy_effect {
        let random = {
            let mut rng = rand::thread_rng();
            rng.gen_range(-1.0..=1.0)
        };

        //cron works minimally on seconds, so we can safely ignore the nanosecond part
        let random_offset_millis = (fuzzy_effect.num_milliseconds() as f64 * random) as i64;

        let random_offset = Duration::milliseconds(random_offset_millis);

        //we don't want to get the final offset in the past
        std::cmp::max(offset_without_fuzzy - random_offset, Duration::zero())
    } else {
        offset_without_fuzzy
    }
}

// becuse of fuzzy offset, we might start a job earlier and complete it before the actual cron trigger was supposed to happen
// so, we might hit the same cron trigger twice, to avoid that we need to sleep if we are too early
pub(super) async fn sleep_if_needed<TZ>(tz: TZ, next: DateTime<TZ>)
where
    TZ: chrono::TimeZone + Send + 'static,
    TZ::Offset: Send,
{
    let new_offset =
        (next - tz.from_utc_datetime(&chrono::Utc::now().naive_utc())).num_milliseconds();
    if new_offset > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(new_offset as u64)).await;
    }
}
