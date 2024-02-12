mod inner;

use crate::error::SchedulerResult;
use crate::job_id::JobId;
use crate::scheduler::inner::SchedulerInner;
use inner::job::{NotStartedJob, StartedJob};
use std::sync::{Arc, Mutex};
use tracing::warn;

/// A scheduler that can schedule jobs to run at specific times.
///
/// The scheduler is thread-safe and can be shared across threads using `.clone()`
#[derive(Clone)]
pub struct Scheduler<TZ>
where
    TZ: chrono::TimeZone + Send + 'static,
    TZ::Offset: Send,
{
    inner: Arc<Mutex<SchedulerInner<TZ>>>,
}

impl Scheduler<chrono::Utc> {
    /// Create a new scheduler that uses UTC timezone
    pub fn new_in_utc() -> Self {
        Self::new(chrono::Utc)
    }
}

impl Scheduler<chrono::Local> {
    /// Create a new scheduler that uses the local timezone
    pub fn new_in_local_timezone() -> Self {
        Self::new(chrono::Local)
    }
}

impl<TZ> Scheduler<TZ>
where
    TZ: chrono::TimeZone + Send + 'static,
    TZ::Offset: Send,
{
    /// Create a new scheduler that uses the specified timezone
    ///
    /// # Arguments
    ///
    /// * `timezone` - The timezone to use
    pub fn new(timezone: TZ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SchedulerInner::new(timezone))),
        }
    }

    /// Start the scheduler with all the jobs that have been scheduled before. Any new job added after this will be started immediately
    ///
    /// Can only be run once!
    ///
    /// # Returns
    ///
    /// A result indicating success or failure for scheduler start
    pub fn start(&self) -> SchedulerResult<()> {
        let mut inner = self.inner.lock().map_err(|err| {
            warn!("Grizzly Scheduler: Mutex error on start: {}", err);
            crate::error::SchedulerError::MutexError(err.to_string())
        })?;
        inner.start();
        Ok(())
    }

    /// Shutdown the scheduler and cancel all the jobs that have been scheduled.
    ///
    /// Can only be run once!
    ///
    /// # Returns
    ///
    /// A result indicating success or failure for scheduler shutdown
    pub fn shutdown(&self) -> SchedulerResult<()> {
        let mut inner = self.inner.lock().map_err(|err| {
            warn!("Grizzly Scheduler: Mutex error on shutdown: {}", err);
            crate::error::SchedulerError::MutexError(err.to_string())
        })?;
        inner.shutdown();
        Ok(())
    }

    /// Cancel a job that has been scheduled
    ///
    /// # Arguments
    ///
    /// * `job_id` - The id of the job to cancel
    ///
    /// # Returns
    ///
    /// A result indicating success or failure for job cancellation
    pub fn cancel_job(&self, job_id: JobId) -> SchedulerResult<()> {
        let mut inner = self.inner.lock().map_err(|err| {
            warn!("Grizzly Scheduler: Mutex error on cancel_job: {}", err);
            crate::error::SchedulerError::MutexError(err.to_string())
        })?;

        inner.cancel_job(job_id)
    }

    /// Schedule a parallel job to run at the specified times. Two instances of this job can run at the same time
    /// if the previous run has not completed and new trigger time has arrived.
    ///
    /// If the scheduler is started, scheduled job will start executing immediately. If it is not started, it will start executing
    /// once the scheduler is started.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use grizzly_scheduler::scheduler::Scheduler;
    ///
    /// let scheduler = grizzly_scheduler::scheduler::Scheduler::new_in_utc();
    /// let important_shared_state = Arc::new(5);
    /// let cloned_state = important_shared_state.clone();
    ///
    /// let job_id = scheduler.schedule_parallel_job(
    ///         "*/5 * * * * *",     // run the job on every second divisible by 5 of every minute
    ///         Some("Example Parallel Job".to_string()), //this name will appear in the tracing logs
    ///         Some(chrono::Duration::seconds(2)), // we want the fuzzy effect of 2 seconds
    ///         move ||
    ///            {
    ///              tracing::info!("We are using our important shared state! {}", cloned_state);
    ///              async move { tracing::info!("In the sequential job",); }
    ///            },
    ///         ).unwrap();
    /// ```
    ///
    /// # Arguments
    ///
    /// * `cron_string` - The cron string to use for scheduling the job
    /// * `human_readable_name` - A human-readable name for the job, will be used in logs
    /// * `fuzzy_offset` - A maximal duration to add or subtract from the trigger time to make the job run at a slightly different time.
    ///                     For example, if the cron is set to run the job on every 30th second of every minute, and the fuzzy offset is set to 5 seconds,
    ///                     the job can run at any point between 25th and 35th second of every minute.
    /// * `f` - The function to run as the job. As this is a parallel job, it can run multiple instances at the same time, so it cannot mutate any captured state.
    ///
    /// # Returns
    ///
    /// A result indicating JobId of scheduled job or failure for job scheduling
    pub fn schedule_parallel_job<F, Fut>(
        &self,
        cron_string: &str,
        human_readable_name: Option<String>,
        fuzzy_offset: Option<chrono::Duration>,
        f: F,
    ) -> SchedulerResult<JobId>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut inner = self.inner.lock().map_err(|err| {
            warn!(
                "Grizzly Scheduler: Mutex error on schedule_parallel_job: {}",
                err
            );
            crate::error::SchedulerError::MutexError(err.to_string())
        })?;

        inner.schedule_parallel_job(cron_string, human_readable_name, fuzzy_offset, f)
    }

    /// Schedule a sequential job to run at the specified times. Only one instance of this job can run at a time, so, if
    /// the previous run has not completed and new trigger time has arrived, the new run will be skipped.
    ///
    /// If the scheduler is started, scheduled job will start executing immediately. If it is not started, it will start executing
    /// once the scheduler is started.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use grizzly_scheduler::scheduler::Scheduler;
    ///
    /// let scheduler = grizzly_scheduler::scheduler::Scheduler::new_in_utc();
    /// let important_shared_state = Arc::new(5);
    /// let cloned_state = important_shared_state.clone();
    ///
    /// let job_id = scheduler.schedule_sequential_job(
    ///         "*/5 * * * * *",     // run the job on every second divisible by 5 of every minute
    ///         Some("Example Sequential Job".to_string()), //this name will appear in the tracing logs
    ///         Some(chrono::Duration::seconds(2)), // we want the fuzzy effect of 2 seconds
    ///         move ||
    ///            {
    ///              tracing::info!("We are using our important shared state! {}", cloned_state);
    ///              async move { tracing::info!("In the sequential job",); }
    ///            },
    ///         ).unwrap();
    /// ```
    ///
    /// # Arguments
    ///
    /// * `cron_string` - The cron string to use for scheduling the job
    /// * `human_readable_name` - A human-readable name for the job, will be used in logs
    /// * `fuzzy_offset` - A maximal duration to add or subtract from the trigger time to make the job run at a slightly different time.
    ///                    For example, if the cron is set to run the job on every 30th second of every minute, and the fuzzy offset is set to 5 seconds,
    ///                   the job can run at any point between 25th and 35th second of every minute.
    /// * `f` - The function to run as the job. As this is a sequential job, it can run only one instance at a time, so it can mutate any captured state.
    ///
    /// # Returns
    ///
    /// A result indicating JobId of scheduled job or failure for job scheduling
    pub fn schedule_sequential_job<F, Fut>(
        &self,
        cron_string: &str,
        human_readable_name: Option<String>,
        fuzzy_offset: Option<chrono::Duration>,
        f: F,
    ) -> SchedulerResult<JobId>
    where
        F: FnMut() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut inner = self.inner.lock().map_err(|err| {
            warn!(
                "Grizzly Scheduler: Mutex error on schedule_sequential_job: {}",
                err
            );
            crate::error::SchedulerError::MutexError(err.to_string())
        })?;

        inner.schedule_sequential_job(cron_string, human_readable_name, fuzzy_offset, f)
    }
}
