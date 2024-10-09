mod impl_parallel;
mod impl_sequential;
pub mod job;
mod utils;

use crate::error::SchedulerError::JobNotFoundError;
use crate::error::SchedulerResult;
use crate::job_id::JobId;
use crate::scheduler::inner::job::{NotStartedJob, StartedJob};
use futures::future::BoxFuture;
use std::collections::HashMap;
use tracing::{debug, info, warn};

type ParallelJobLambda = Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;
type SequentialJobLambda = Box<dyn FnMut() -> BoxFuture<'static, ()> + Send>;

pub struct SchedulerInner<TZ>
where
    TZ: chrono::TimeZone + Send + 'static,
    TZ::Offset: Send,
{
    started: bool,
    terminated: bool,
    started_jobs: HashMap<String, StartedJob>,
    not_started_parallel_jobs: HashMap<String, NotStartedJob<ParallelJobLambda>>,
    not_started_sequential_jobs: HashMap<String, NotStartedJob<SequentialJobLambda>>,
    timezone: TZ,
}

impl<TZ> SchedulerInner<TZ>
where
    TZ: chrono::TimeZone + Send + 'static,
    TZ::Offset: Send,
{
    pub(super) fn new(timezone: TZ) -> Self {
        Self {
            started_jobs: HashMap::new(),
            not_started_parallel_jobs: HashMap::new(),
            not_started_sequential_jobs: HashMap::new(),
            timezone,
            started: false,
            terminated: false,
        }
    }

    pub(super) fn start(&mut self) {
        let scheduled_parallel_job_ids: Vec<String> =
            self.not_started_parallel_jobs.keys().cloned().collect();
        let scheduled_sequential_job_ids: Vec<String> =
            self.not_started_sequential_jobs.keys().cloned().collect();

        info!(
            scheduled_parallel_job_ids = format!("{:?}", scheduled_parallel_job_ids),
            scheduled_sequential_job_ids = format!("{:?}", scheduled_sequential_job_ids),
            "Grizzly Scheduler: Starting scheduler"
        );
        self.started = true;

        self.start_non_started_parallel_jobs();
        self.start_non_started_sequential_jobs();
    }

    pub(super) fn shutdown(&mut self) {
        info!("Grizzly Scheduler: Shutting down scheduler");
        if self.terminated {
            return;
        }
        self.terminated = true;

        for (job_id, started_job) in self.started_jobs.drain() {
            debug!(
                "Grizzly Scheduler: Scheduler shutdown - cancelling job with id {}",
                job_id
            );
            started_job.cancellation_token.cancel();
        }
    }

    pub(super) fn cancel_job(&mut self, job_id: JobId) -> SchedulerResult<()> {
        if let Some(started_job) = self.started_jobs.remove(job_id.as_str()) {
            debug!(
                "Grizzly Scheduler: Cancelling job with id {} and human readable name {}",
                job_id.as_str(),
                started_job
                    .human_readable_name
                    .as_deref()
                    .unwrap_or("NOT SET")
            );
            started_job.cancellation_token.cancel();
            Ok(())
        } else {
            warn!(
                "Grizzly Scheduler: Job with id {} not found for cancellation",
                job_id.as_str()
            );
            Err(JobNotFoundError(job_id.as_str().to_string()))
        }
    }

    pub(super) fn stop_all_jobs(&mut self) {
        info!("Grizzly Scheduler: Stopping all jobs");
        for (job_id, started_job) in self.started_jobs.iter() {
            debug!(
                "Grizzly Scheduler: Stopping job with id {} and human readable name {}",
                job_id,
                started_job
                    .human_readable_name
                    .as_deref()
                    .unwrap_or("NOT SET")
            );
            started_job.cancellation_token.cancel();
        }
    }

    pub(super) fn stop_jobs_by_category(&mut self, category: &str) {
        info!("Grizzly Scheduler: Stopping jobs in category: {}", category);
        let job_ids_to_stop: Vec<String> = self
            .started_jobs
            .iter()
            .filter(|(_, started_job)| started_job.category.as_deref() == Some(category))
            .map(|(job_id, _)| job_id.clone())
            .collect();

        for job_id in job_ids_to_stop {
            if let Some(started_job) = self.started_jobs.remove(&job_id) {
                debug!(
                    "Grizzly Scheduler: Stopping job with id {} and human readable name {}",
                    job_id,
                    started_job
                        .human_readable_name
                        .as_deref()
                        .unwrap_or("NOT SET")
                );
                started_job.cancellation_token.cancel();
            }
        }
    }
}

impl<TZ> Drop for SchedulerInner<TZ>
where
    TZ: chrono::TimeZone + Send + 'static,
    TZ::Offset: Send,
{
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_new() {
        let scheduler = SchedulerInner::<Utc>::new(Utc);
        assert!(!scheduler.started);
        assert!(!scheduler.terminated);
        assert!(scheduler.started_jobs.is_empty());
        assert!(scheduler.not_started_parallel_jobs.is_empty());
        assert!(scheduler.not_started_sequential_jobs.is_empty());
    }
}
