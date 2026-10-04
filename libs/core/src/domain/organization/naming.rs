//! Validated names and slugs.
//!
//! Both arrived as bare `String`s and were inserted as given: no trimming, no
//! length, no format. A slug is destined for a URL, so an empty one, one made
//! of spaces, or one several megabytes long is a routing problem waiting to
//! happen. Parsing at the edge into a type the rest of the code can trust is
//! cheaper than checking at every use.

use std::fmt::Display;

use serde::Serialize;

const SLUG_MIN: usize = 2;
const SLUG_MAX: usize = 63;
const NAME_MAX: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamingError {
    Empty(&'static str),
    TooShort { field: &'static str, min: usize },
    TooLong { field: &'static str, max: usize },
    SlugCharset,
    SlugEdge,
}

impl Display for NamingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty(field) => write!(f, "{field} must not be empty"),
            Self::TooShort { field, min } => {
                write!(f, "{field} must be at least {min} characters")
            }
            Self::TooLong { field, max } => write!(f, "{field} must be at most {max} characters"),
            Self::SlugCharset => {
                write!(
                    f,
                    "slug may only contain lowercase letters, digits and hyphens"
                )
            }
            Self::SlugEdge => write!(f, "slug must start and end with a letter or digit"),
        }
    }
}

/// A URL-safe organization slug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Slug(String);

impl Slug {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl TryFrom<String> for Slug {
    type Error = NamingError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        let value = raw.trim();

        if value.is_empty() {
            return Err(NamingError::Empty("slug"));
        }
        if value.len() < SLUG_MIN {
            return Err(NamingError::TooShort {
                field: "slug",
                min: SLUG_MIN,
            });
        }
        if value.len() > SLUG_MAX {
            return Err(NamingError::TooLong {
                field: "slug",
                max: SLUG_MAX,
            });
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(NamingError::SlugCharset);
        }
        // A leading or trailing hyphen reads as a separator with nothing on one
        // side of it, and collides visually with the disambiguating suffixes a
        // generated slug may carry.
        let edges_are_alphanumeric = value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
            && value
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric());
        if !edges_are_alphanumeric {
            return Err(NamingError::SlugEdge);
        }

        Ok(Self(value.to_owned()))
    }
}

impl Display for Slug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A human-facing organization name. Trimmed and bounded; deliberately not
/// restricted in charset, since it carries a real business name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrganizationName(String);

impl OrganizationName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl TryFrom<String> for OrganizationName {
    type Error = NamingError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        let value = raw.trim();

        if value.is_empty() {
            return Err(NamingError::Empty("name"));
        }
        if value.chars().count() > NAME_MAX {
            return Err(NamingError::TooLong {
                field: "name",
                max: NAME_MAX,
            });
        }

        Ok(Self(value.to_owned()))
    }
}

impl Display for OrganizationName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug(raw: &str) -> Result<Slug, NamingError> {
        Slug::try_from(raw.to_owned())
    }

    #[test]
    fn a_plain_slug_is_accepted_and_trimmed() {
        assert_eq!(slug("  acme-corp  ").unwrap().as_str(), "acme-corp");
    }

    #[test]
    fn an_empty_or_blank_slug_is_rejected() {
        assert_eq!(slug("").unwrap_err(), NamingError::Empty("slug"));
        assert_eq!(slug("   ").unwrap_err(), NamingError::Empty("slug"));
    }

    #[test]
    fn a_slug_cannot_carry_path_separators() {
        // The slug ends up in a URL; "../admin" must never reach routing.
        assert_eq!(slug("../admin").unwrap_err(), NamingError::SlugCharset);
        assert_eq!(slug("a/b").unwrap_err(), NamingError::SlugCharset);
    }

    #[test]
    fn a_slug_cannot_carry_spaces_or_uppercase() {
        assert_eq!(slug("acme corp").unwrap_err(), NamingError::SlugCharset);
        assert_eq!(slug("AcmeCorp").unwrap_err(), NamingError::SlugCharset);
    }

    #[test]
    fn a_slug_cannot_start_or_end_with_a_hyphen() {
        assert_eq!(slug("-acme").unwrap_err(), NamingError::SlugEdge);
        assert_eq!(slug("acme-").unwrap_err(), NamingError::SlugEdge);
    }

    #[test]
    fn an_unbounded_slug_is_rejected() {
        let long = "a".repeat(SLUG_MAX + 1);
        assert!(matches!(
            slug(&long).unwrap_err(),
            NamingError::TooLong { .. }
        ));
    }

    #[test]
    fn a_name_is_trimmed_and_bounded_but_not_restricted() {
        let name = OrganizationName::try_from("  Boulangerie Étoile  ".to_owned()).unwrap();
        assert_eq!(name.as_str(), "Boulangerie Étoile");

        let long = "é".repeat(NAME_MAX + 1);
        assert!(matches!(
            OrganizationName::try_from(long).unwrap_err(),
            NamingError::TooLong { .. }
        ));
    }
}

impl Slug {
    /// Derives a slug from arbitrary text — a username, an email local part.
    ///
    /// Returns `None` when nothing usable survives, rather than inventing a
    /// value: a caller that cannot name an organization should say so instead
    /// of creating one called `x`.
    pub fn from_seed(seed: &str) -> Option<Self> {
        let mut out = String::with_capacity(seed.len());
        let mut last_was_hyphen = false;

        for ch in seed.chars() {
            let mapped = if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            };
            if mapped == '-' {
                // Collapse runs, and never start with one.
                if last_was_hyphen || out.is_empty() {
                    continue;
                }
                last_was_hyphen = true;
            } else {
                last_was_hyphen = false;
            }
            out.push(mapped);
            if out.len() >= SLUG_MAX {
                break;
            }
        }

        let trimmed = out.trim_end_matches('-');
        Self::try_from(trimmed.to_owned()).ok()
    }

    /// The same slug with a disambiguating suffix, for when the base is taken.
    /// Truncates the stem if needed so the result still fits.
    pub fn with_suffix(&self, n: u32) -> Option<Self> {
        let suffix = format!("-{n}");
        let room = SLUG_MAX.saturating_sub(suffix.len());
        let stem = self.0.get(..room.min(self.0.len()))?.trim_end_matches('-');
        Self::try_from(format!("{stem}{suffix}")).ok()
    }
}

#[cfg(test)]
mod seed_tests {
    use super::*;

    #[test]
    fn a_username_becomes_a_usable_slug() {
        assert_eq!(Slug::from_seed("Nathaël B.").unwrap().as_str(), "natha-l-b");
        assert_eq!(Slug::from_seed("acme_corp").unwrap().as_str(), "acme-corp");
    }

    #[test]
    fn separators_collapse_and_never_sit_on_an_edge() {
        assert_eq!(Slug::from_seed("  --acme--  ").unwrap().as_str(), "acme");
    }

    #[test]
    fn a_seed_with_nothing_usable_yields_nothing() {
        // Better than inventing a name the user never chose.
        assert!(Slug::from_seed("").is_none());
        assert!(Slug::from_seed("...").is_none());
        assert!(Slug::from_seed("x").is_none()); // below the minimum length
    }

    #[test]
    fn a_long_seed_is_truncated_to_a_valid_slug() {
        let slug = Slug::from_seed(&"a".repeat(200)).unwrap();
        assert_eq!(slug.as_str().len(), SLUG_MAX);
    }

    #[test]
    fn a_suffix_keeps_the_result_within_bounds() {
        let base = Slug::from_seed(&"b".repeat(200)).unwrap();
        let suffixed = base.with_suffix(12).unwrap();
        assert!(suffixed.as_str().len() <= SLUG_MAX);
        assert!(suffixed.as_str().ends_with("-12"));
    }
}
