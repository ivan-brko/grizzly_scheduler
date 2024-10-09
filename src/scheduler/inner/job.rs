use cron::Schedule;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(in crate::scheduler) struct StartedJob {
    pub cancellation_token: CancellationToken,
    #[allow(dead_code)]
    pub join: JoinHandle<()>,
    pub human_readable_name: Option<String>,
    pub category: Option<String>,
}

pub(in crate::scheduler) struct NotStartedJob<T> {
    pub parsed_cron: Schedule,
    pub lambda: T,
    pub human_readable_name: Option<String>,
    pub category: Option<String>,
    pub fuzzy_offset: Option<chrono::Duration>,
}
