use std::{collections::HashMap, sync::Mutex};

use chrono::{DateTime, TimeZone, Utc};
use lodestone_core::{
    CompanyDossier, DnsError, DnsRecords, DnsResolver, EcosystemFinding, FetchError, FetchRequest,
    FetchResponse, Fetcher, Hostname, InspectionSettings, InspectionTarget, NormalizedDomain,
    SirenLookup,
};

pub type Reply = Result<(u16, Vec<(&'static str, &'static str)>, String), FetchError>;

pub struct FakeFetcher {
    script: HashMap<String, Reply>,
    requests: Mutex<Vec<String>>,
}

impl FakeFetcher {
    pub fn new(script: Vec<(&str, Reply)>) -> Self {
        Self {
            script: script
                .into_iter()
                .map(|(url, reply)| (url.to_owned(), reply))
                .collect(),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn requested(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Fetcher for FakeFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let key = request.url.to_string();
        self.requests.lock().unwrap().push(key.clone());
        match self.script.get(&key) {
            Some(Ok((status, headers, body))) => Ok(FetchResponse {
                status: *status,
                headers: headers
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                    .collect(),
                body: body.clone().into_bytes(),
                final_url: request.url.clone(),
                truncated: false,
            }),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(FetchResponse {
                status: 404,
                headers: Vec::new(),
                body: Vec::new(),
                final_url: request.url.clone(),
                truncated: false,
            }),
        }
    }
}

#[derive(Default)]
pub struct FakeResolver {
    pub mx: HashMap<String, Result<Vec<&'static str>, DnsError>>,
    pub txt: HashMap<String, Result<Vec<&'static str>, DnsError>>,
    pub cname: HashMap<String, Result<Vec<&'static str>, DnsError>>,
}

fn hosts(names: &[&str]) -> Vec<Hostname> {
    names
        .iter()
        .filter_map(|name| Hostname::parse(name).ok())
        .collect()
}

impl DnsRecords for FakeResolver {
    async fn mx_hosts(&self, domain: &NormalizedDomain) -> Result<Vec<Hostname>, DnsError> {
        match self.mx.get(domain.as_str()) {
            Some(Ok(names)) => Ok(hosts(names)),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(Vec::new()),
        }
    }

    async fn txt_records(&self, name: &str) -> Result<Vec<String>, DnsError> {
        match self.txt.get(name) {
            Some(Ok(records)) => Ok(records.iter().map(|record| (*record).to_owned()).collect()),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(Vec::new()),
        }
    }
}

impl DnsResolver for FakeResolver {
    async fn cname_chain(&self, host: &Hostname) -> Result<Vec<Hostname>, DnsError> {
        match self.cname.get(host.as_str()) {
            Some(Ok(names)) => Ok(hosts(names)),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(Vec::new()),
        }
    }
}

pub fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap()
}

pub fn domain() -> NormalizedDomain {
    NormalizedDomain::from_host("acme.fr").unwrap()
}

pub fn html(body: &str) -> Reply {
    Ok((
        200,
        vec![("content-type", "text/html"), ("server", "nginx")],
        body.to_owned(),
    ))
}

pub fn xml(body: &str) -> Reply {
    Ok((
        200,
        vec![("content-type", "application/xml")],
        body.to_owned(),
    ))
}

pub fn target<'a>(
    domain: &'a NormalizedDomain,
    settings: &'a InspectionSettings,
    dossier: Option<&'a CompanyDossier>,
) -> InspectionTarget<'a> {
    with_previous(domain, settings, dossier, &[])
}

pub fn with_previous<'a>(
    domain: &'a NormalizedDomain,
    settings: &'a InspectionSettings,
    dossier: Option<&'a CompanyDossier>,
    previous: &'a [EcosystemFinding],
) -> InspectionTarget<'a> {
    InspectionTarget {
        domain,
        settings,
        dossier,
        siren: SirenLookup::Unresolved,
        previous,
        now: now(),
    }
}
