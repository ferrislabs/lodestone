use std::{
    error::Error,
    io::{self, BufRead},
};

use args::database::DatabaseArgs;
use chrono::Utc;
use clap::Parser;
use dotenvy::dotenv;
use lodestone_core::{
    CandidateHost, CandidateHosts, HostOrigin, Hostname,
    infrastructure::candidate_host::PgCandidateHosts,
};
use sqlx::postgres::PgPoolOptions;

const BATCH_SIZE: usize = 1000;

#[derive(Parser, Debug)]
#[command(name = "import-hosts", about = "Imports hostnames read from stdin")]
struct ImportArgs {
    #[command(flatten)]
    database: DatabaseArgs,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let args = ImportArgs::parse();
    let database = &args.database;
    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        database.user,
        database.password.expose(),
        database.host,
        database.port,
        database.name,
    );
    let pool = PgPoolOptions::new().connect(&url).await?;
    let hosts = PgCandidateHosts::new(pool);
    let now = Utc::now();

    let mut imported: u64 = 0;
    let mut rejected: u64 = 0;
    let mut batch: Vec<CandidateHost> = Vec::with_capacity(BATCH_SIZE);
    for line in io::stdin().lock().lines() {
        let line = line?;
        let entry = line.trim();
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        let candidate = Hostname::parse(entry)
            .and_then(|host| CandidateHost::discovered(host, HostOrigin::Import, now));
        match candidate {
            Ok(candidate) => batch.push(candidate),
            Err(_) => {
                rejected += 1;
                eprintln!("rejected: {entry}");
            }
        }
        if batch.len() >= BATCH_SIZE {
            imported += hosts.upsert(&batch).await?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        imported += hosts.upsert(&batch).await?;
    }
    println!("imported={imported} rejected={rejected}");
    Ok(())
}
