use std::fmt::{Debug, Display, Formatter, Result as FmtResult};

/// A configuration value that must never reach a log or an error message.
///
/// The structs carrying secrets all derive `Debug`, and any future
/// `debug!(?config)` or panic message including one would have printed them in
/// full. Wrapping makes that impossible rather than merely unlikely: the value
/// is only reachable through [`Secret::expose`], which is greppable.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Yields the underlying value. Named so that a review can find every
    /// place a secret leaves its wrapper.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl Debug for Secret {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("Secret([redacted])")
    }
}

impl Display for Secret {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("[redacted]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_never_reveal_the_value() {
        let secret = Secret::new("hunter2");
        assert!(!format!("{secret:?}").contains("hunter2"));
        assert!(!format!("{secret}").contains("hunter2"));
        assert_eq!(secret.expose(), "hunter2");
    }

    #[test]
    fn a_struct_deriving_debug_cannot_leak_it() {
        #[derive(Debug)]
        #[allow(dead_code)] // read only through the derived Debug impl
        struct Config {
            password: Secret,
        }

        let printed = format!(
            "{:?}",
            Config {
                password: Secret::new("hunter2")
            }
        );
        assert!(!printed.contains("hunter2"));
    }
}
