use serde::{Deserialize, Serialize};

use crate::domain::llm::{GenerationParams, ReasoningEffort, provider::ProviderId};

pub const DEFAULT_SAMPLES: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct GenerationSnapshot {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub repeat_penalty: Option<f64>,
    pub reasoning_effort: Option<ReasoningEffort>,
}

impl GenerationSnapshot {
    pub fn new(
        generation: &GenerationParams,
        reasoning_effort: Option<ReasoningEffort>,
        default_max_tokens: u32,
    ) -> Self {
        Self {
            temperature: generation.temperature,
            max_tokens: Some(generation.max_tokens_or(default_max_tokens)),
            frequency_penalty: generation.frequency_penalty,
            presence_penalty: generation.presence_penalty,
            repeat_penalty: generation.repeat_penalty,
            reasoning_effort,
        }
    }
}

fn default_samples() -> u32 {
    DEFAULT_SAMPLES
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunProvenance {
    pub provider_id: Option<ProviderId>,
    pub provider_name: String,
    pub model: String,
    pub prompt_version: String,
    pub generation: GenerationSnapshot,
    #[serde(default = "default_samples")]
    pub samples: u32,
}

impl RunProvenance {
    pub fn unresolved(provider_id: Option<ProviderId>, prompt_version: String) -> Self {
        Self {
            provider_id,
            provider_name: String::new(),
            model: String::new(),
            prompt_version,
            generation: GenerationSnapshot::default(),
            samples: DEFAULT_SAMPLES,
        }
    }

    pub fn legacy(model: String, prompt_version: String) -> Self {
        Self {
            model,
            ..Self::unresolved(None, prompt_version)
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn provenance() -> RunProvenance {
        RunProvenance {
            provider_id: Some(ProviderId(Uuid::nil())),
            provider_name: "local".to_owned(),
            model: "qwen3".to_owned(),
            prompt_version: "qualification-agent@abc".to_owned(),
            generation: GenerationSnapshot {
                temperature: Some(0.2),
                max_tokens: Some(2048),
                frequency_penalty: None,
                presence_penalty: Some(0.5),
                repeat_penalty: Some(1.1),
                reasoning_effort: Some(ReasoningEffort::Low),
            },
            samples: 1,
        }
    }

    #[test]
    fn provenance_survives_a_json_round_trip() {
        let json = serde_json::to_value(provenance()).unwrap();
        assert_eq!(
            serde_json::from_value::<RunProvenance>(json).unwrap(),
            provenance()
        );
    }

    #[test]
    fn samples_default_to_one_when_absent() {
        let mut json = serde_json::to_value(provenance()).unwrap();
        json.as_object_mut().unwrap().remove("samples");
        assert_eq!(
            serde_json::from_value::<RunProvenance>(json)
                .unwrap()
                .samples,
            1
        );
    }

    #[test]
    fn the_snapshot_records_the_effective_max_tokens() {
        let params = GenerationParams {
            temperature: Some(0.3),
            ..GenerationParams::default()
        };
        let snapshot = GenerationSnapshot::new(&params, None, 4096);
        assert_eq!(snapshot.max_tokens, Some(4096));
        assert_eq!(snapshot.temperature, Some(0.3));
        let explicit = GenerationParams {
            max_tokens: Some(512),
            ..params
        };
        assert_eq!(
            GenerationSnapshot::new(&explicit, None, 4096).max_tokens,
            Some(512)
        );
    }
}
