use clap::Parser;
use common::{Config, SettingsError};

use crate::{
    auth::AuthArgs, contacts::ContactArgs, ct::CtArgs, database::DatabaseArgs, jobs::JobsArgs,
    llm::LlmArgs, log::LogArgs, observability::ObservabilityArgs, outbound::OutboundArgs,
    rate_limit::RateLimitArgs, server::ServerArgs,
};

pub mod auth;
pub mod contacts;
pub mod ct;
pub mod database;
pub mod jobs;
pub mod llm;
pub mod log;
pub mod observability;
pub mod outbound;
pub mod rate_limit;
pub mod server;

#[derive(Debug, Clone, Parser)]
pub struct Args {
    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub db: DatabaseArgs,

    #[command(flatten)]
    pub auth: AuthArgs,

    #[command(flatten)]
    pub server: ServerArgs,

    #[command(flatten)]
    pub observability: ObservabilityArgs,

    #[command(flatten)]
    pub rate_limit: RateLimitArgs,

    #[command(flatten)]
    pub jobs: JobsArgs,

    #[command(flatten)]
    pub outbound: OutboundArgs,

    #[command(flatten)]
    pub llm: LlmArgs,

    #[command(flatten)]
    pub ct: CtArgs,

    #[command(flatten)]
    pub contacts: ContactArgs,
}

impl TryFrom<Args> for Config {
    type Error = SettingsError;

    fn try_from(value: Args) -> Result<Self, Self::Error> {
        Ok(Self {
            auth: value.auth.into(),
            database: value.db.into(),
            rate_limit: value.rate_limit.into(),
            jobs: value.jobs.try_into()?,
            outbound: value.outbound.try_into()?,
            llm: value.llm.try_into()?,
        })
    }
}
