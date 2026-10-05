use clap::Parser;
use common::{Config, SettingsError};

use crate::{
    auth::AuthArgs, boamp::BoampArgs, contacts::ContactArgs, ct::CtArgs, database::DatabaseArgs,
    decp::DecpArgs, jobs::JobsArgs, llm::LlmArgs, log::LogArgs, observability::ObservabilityArgs,
    outbound::OutboundArgs, rate_limit::RateLimitArgs, server::ServerArgs, ted::TedArgs,
};

pub mod auth;
pub mod boamp;
pub mod contacts;
pub mod ct;
pub mod database;
pub mod decp;
pub mod jobs;
pub mod llm;
pub mod log;
pub mod observability;
pub mod outbound;
pub mod rate_limit;
pub mod server;
pub mod ted;

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

    #[command(flatten)]
    pub boamp: BoampArgs,

    #[command(flatten)]
    pub decp: DecpArgs,

    #[command(flatten)]
    pub ted: TedArgs,
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
