use lodestone_core::{
    Job,
    jobs::{JobError, JobHandler},
};

pub struct JobRouter;

impl JobHandler for JobRouter {
    async fn handle(&self, job: &Job) -> Result<(), JobError> {
        Err(JobError::Permanent(format!(
            "no handler for job kind `{}`",
            job.kind.as_str()
        )))
    }
}
