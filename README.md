# Grizzly Cron Scheduler

[![](https://docs.rs/grizzly_scheduler/badge.svg)](https://docs.rs/grizzly_scheduler) [![](https://img.shields.io/crates/v/grizzly_scheduler.svg)](https://crates.io/crates/grizzly_scheduler)

A simple and easy to use scheduler, built on top of Tokio, that allows you to schedule async tasks using cron expressions (with optional random fuzzy offsets for each trigger).

Tasks can be of two types:
- Parallel: Two instances of this job can run at the same time if the previous run has not completed and new trigger time has arrived.
- Sequential: The next run of the job will not start until the previous run has completed, so some triggers might be skipped.

## Alternatives
- [tokio cron scheduler](https://github.com/mvniekerk/tokio-cron-scheduler): Well-maintained and feature-rich, but it does not support fuzzy scheduling or differentiate between parallel and sequential jobs.

## Features

- Schedule parallel and sequential jobs
- Schedule jobs using cron expressions
- Integrated with [Tracing](https://docs.rs/tracing/latest/tracing/) for logging
- Add random fuzzy offset to each trigger time
  - for example, if the job is configured to run at 30th second of every minute, you can add a fuzzy offset of 5 seconds, so the job will run at a random point between 25th and 35th second of every minute

## Example

```rust
use std::sync::Arc;
use grizzly_scheduler::scheduler::Scheduler;

let scheduler = grizzly_scheduler::scheduler::Scheduler::new_in_utc();
let important_shared_state = Arc::new(5);
let cloned_state = important_shared_state.clone();

let job_id = scheduler.schedule_parallel_job(
       "*/5 * * * * *",                          // run the job on every second divisible by 5 of every minute
       Some("Example Parallel Job".to_string()), // this name will appear in the tracing logs
       Some(chrono::Duration::seconds(2)),       // we want the fuzzy effect of maximally +/-2 seconds
       move ||
          {
             let cloned_state = cloned_state.clone();
             async move {
                 tracing::info!("We are using our important shared state! {}", cloned_state);
             }
          },
       ).unwrap();
scheduler.start().unwrap();
```

 ## License
 This project is licensed under MIT license, which can be found in the LICENSE file.

## Contribution
 All contributions are welcome! Please feel free to open an issue or a pull request.

