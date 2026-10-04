use std::num::NonZeroU32;

use common::{EncryptionKey, LlmConfig, Secret, SettingsError};

#[derive(clap::Args, Debug, Clone)]
pub struct LlmArgs {
    #[arg(
        long = "llm-secrets-encryption-key",
        env = "LLM_SECRETS_ENCRYPTION_KEY",
        name = "LLM_SECRETS_ENCRYPTION_KEY",
        hide_env_values = true,
        long_help = "Base64 of 32 random bytes encrypting LLM provider API keys at rest"
    )]
    pub secrets_encryption_key: Secret,

    #[arg(
        long = "llm-run-budget-cents",
        env = "LLM_RUN_BUDGET_CENTS",
        name = "LLM_RUN_BUDGET_CENTS",
        default_value = "500",
        long_help = "Maximum LLM spend of one run, in US cents"
    )]
    pub run_budget_cents: NonZeroU32,
}

impl TryFrom<LlmArgs> for LlmConfig {
    type Error = SettingsError;

    fn try_from(value: LlmArgs) -> Result<Self, Self::Error> {
        Ok(Self {
            encryption_key: EncryptionKey::from_base64(&value.secrets_encryption_key)?,
            run_budget_cents: value.run_budget_cents,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        llm: LlmArgs,
    }

    const VALID_KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

    #[test]
    fn key_is_required() {
        assert!(Cmd::try_parse_from(["cmd"]).is_err());
    }

    #[test]
    fn valid_key_builds_a_config() {
        let cmd = Cmd::try_parse_from(["cmd", "--llm-secrets-encryption-key", VALID_KEY]).unwrap();
        let config = LlmConfig::try_from(cmd.llm).unwrap();
        assert_eq!(config.run_budget_cents.get(), 500);
        assert_eq!(config.encryption_key.expose()[31], 31);
    }

    #[test]
    fn malformed_key_is_rejected() {
        let cmd = Cmd::try_parse_from(["cmd", "--llm-secrets-encryption-key", "c2hvcnQ="]).unwrap();
        assert_eq!(
            LlmConfig::try_from(cmd.llm).unwrap_err(),
            SettingsError::InvalidEncryptionKey
        );
    }

    #[test]
    fn parsed_args_never_print_the_key() {
        let cmd = Cmd::try_parse_from(["cmd", "--llm-secrets-encryption-key", VALID_KEY]).unwrap();
        assert!(!format!("{:?}", cmd.llm).contains(VALID_KEY));
    }
}
