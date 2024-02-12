/// Error type returned by the scheduler
#[derive(thiserror::Error, Debug)]
pub enum SchedulerError {
    //cron lib is not updated, we might need to use some other lib in the future,
    //so we're not using transparent error to avoid breaking changes
    #[error("Invalid cron expression: {0}")]
    CronParseError(String),
    #[error("Mutex error: {0}")]
    MutexError(String),
    #[error("Job not found for id: {0}")]
    JobNotFoundError(String),
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;
