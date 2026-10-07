use common::Secret;
use serde_json::{Map, Value};

#[derive(Debug, Clone)]
pub enum Credential {
    None,
    Header { name: String, value: Secret },
    Bearer(Secret),
}

#[derive(Debug)]
pub struct ProbeContext<'a> {
    pub settings: &'a Map<String, Value>,
    pub credential: &'a Credential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub ok: bool,
    pub message: String,
}

impl ProbeOutcome {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

pub trait ProviderProbe: Send + Sync {
    fn probe(&self, context: &ProbeContext<'_>) -> impl Future<Output = ProbeOutcome> + Send;
}
