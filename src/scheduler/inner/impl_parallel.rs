use super::utils;
use crate::error::SchedulerResult;
use crate::job_id::JobId;
use crate::scheduler::{NotStartedJob, SchedulerInner, StartedJob};
use futures::FutureExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, info_span, Instrument};

impl<TZ> SchedulerInner<TZ>
where
    TZ: chrono::TimeZone + Send + 'static,
    TZ::Offset: Send,
{
    pub(crate) fn schedule_parallel_job<F, Fut>(
        &mut self,
        cron_string: &str,
        human_readable_name: Option<String>,
        category: Option<String>,
        fuzzy_offset: Option<chrono::Duration>,
        f: F,
    ) -> SchedulerResult<JobId>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let job_id = utils::generate_job_id();

        let parsed_cron = utils::parse_cron(cron_string)?;

        debug!(
            "Grizzly Scheduler: Scheduling parallel job with id {} and cron schedule {}",
            job_id, cron_string
        );

        self.not_started_parallel_jobs.insert(
            job_id.clone(),
            NotStartedJob {
                human_readable_name,
                category,
                parsed_cron,
                lambda: Box::new(move || f().boxed()),
                fuzzy_offset,
            },
        );

        if self.started {
            self.start_non_started_parallel_jobs();
        }

        Ok(JobId::new(job_id))
    }

    pub(super) fn start_non_started_parallel_jobs(&mut self) {
        debug!("Grizzly Scheduler: Starting non-started parallel jobs");
        for (job_id, job) in self.not_started_parallel_jobs.drain() {
            let span = if let Some(hrn) = job.human_readable_name.as_ref() {
                info!(
                    "Grizzly Scheduler: Starting parallel job {} with id {}",
                    hrn, job_id
                );
                info_span!("Grizzly Scheduler Parallel Job", job_id = %job_id, human_readable_name = %hrn)
            } else {
                info!(
                    "Grizzly Scheduler: Starting parallel job with id {}",
                    job_id
                );
                info_span!("Grizzly Scheduler Parallel Job", job_id = %job_id)
            };
            let lambda = Arc::new(job.lambda);
            let tz = self.timezone.clone();
            let cancellation_token = CancellationToken::new();
            let cloned_token = cancellation_token.clone();
            let tz_cloned = tz.clone();
            let join = tokio::spawn(async move {
                'execution_loop: for next in job.parsed_cron.upcoming(tz.clone()) {
                    let next_cloned = next.clone();
                    let initial_offset = next - tz.from_utc_datetime(&chrono::Utc::now().naive_utc());
                    let final_offset = utils::maybe_with_fuzzy_offset(initial_offset, job.fuzzy_offset);

                    let offset_millis = final_offset.num_milliseconds();
                    if offset_millis < 0 {
                        continue 'execution_loop;
                    }
                    let cloned_cloned_token = cloned_token.clone();
                    let cloned_lambda = lambda.clone();
                    let tz_cloned = tz_cloned.clone();
                    select! {
                            _ = async {
                                tokio::time::sleep(Duration::from_millis(offset_millis as u64)).await;
                                let single_run_id = utils::generate_single_run_id();
                                let span = info_span!("Grizzly Parallel Job Single Run", single_run_id = %single_run_id);
                                tokio::spawn(async move {
                                    select! {
                                        _ = cloned_cloned_token.cancelled() => {
                                            info!("Grizzly Scheduler: Job was cancelled during execution");
                                        }
                                        _ = async move{
                                            debug!("Grizzly Scheduler: Starting parallel job single run");
                                            cloned_lambda().await;
                                        } => {
                                            debug!("Grizzly Scheduler: Completed parallel job single run");
                                        }
                                    }
                                }.instrument(span));
                                utils::sleep_if_needed(tz_cloned, next_cloned).await;
                            }
                             => {},
                            _ = cloned_token.cancelled() => {
                            info!("Grizzly Scheduler: Job was cancelled during scheduling");
                            break 'execution_loop;
                        }
                    }
                }
            }.instrument(span));

            self.started_jobs.insert(
                job_id,
                StartedJob {
                    cancellation_token,
                    join,
                    human_readable_name: job.human_readable_name,
                    category: job.category,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    // this is a simple scenario of parallel task, where we have a counter that is incremented every 2 seconds
    // and every run is short so there are no overlapping runs
    #[tokio::test]
    async fn test_simple_multiple_triggers_atomic_increment() {
        let mut scheduler = SchedulerInner::<Utc>::new(Utc);

        let counter = Arc::new(AtomicUsize::new(0));
        let c_counter = Arc::clone(&counter);

        // Job that increments the counter every 2 seconds
        scheduler
            .schedule_parallel_job("*/2 * * * * *", None, None, None, move || {
                let cc_counter = Arc::clone(&c_counter);
                async move {
                    cc_counter.fetch_add(1, Ordering::Relaxed);
                }
            })
            .unwrap();

        // Start the scheduler
        scheduler.start();

        // Let the jobs run for a while
        sleep(Duration::from_secs(9)).await;

        // Stop the scheduler
        scheduler.shutdown();

        let call_count = counter.load(Ordering::Relaxed);

        //depending on the moment first call was scheduled this can be either 4 or 5
        assert!(call_count == 4 || call_count == 5);
    }

    // this is a more complex scenario of parallel task, where we have a counter that is incremented every 2 seconds
    // and every run actually takes 5 seconds, so we're verifying that the runs are really happening in parallel
    #[tokio::test]
    async fn test_complex_multiple_triggers_atomic_increment() {
        let mut scheduler = SchedulerInner::<Utc>::new(Utc);

        let counter = Arc::new(AtomicUsize::new(0));
        let c_counter = Arc::clone(&counter);

        // Job that increments the counter every 2 seconds
        scheduler
            .schedule_parallel_job("*/2 * * * * *", None, None, None, move || {
                let cc_counter = Arc::clone(&c_counter);
                async move {
                    cc_counter.fetch_add(1, Ordering::SeqCst);
                    sleep(Duration::from_secs(5)).await;
                }
            })
            .unwrap();

        // Start the scheduler
        scheduler.start();

        // Let the jobs run for a while
        sleep(Duration::from_secs(9)).await;

        // Stop the scheduler
        scheduler.shutdown();

        let call_count = counter.load(Ordering::Relaxed);

        //depending on the moment first call was scheduled this can be either 4 or 5
        assert!(call_count == 4 || call_count == 5);
    }
}
