use crate::domain::lead::str_enum;

pub const MAX_NOTE_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RejectionReason {
    OffTarget,
    AlreadyCustomer,
    Competitor,
    VendorOrIntegrator,
    HostOrAgency,
    BadData,
    Other,
}

str_enum!(RejectionReason {
    OffTarget => "hors_cible",
    AlreadyCustomer => "deja_client",
    Competitor => "concurrent",
    VendorOrIntegrator => "editeur_integrateur",
    HostOrAgency => "hebergeur_agence",
    BadData => "mauvaise_donnee",
    Other => "autre",
});

impl RejectionReason {
    pub const ALL: [Self; 7] = [
        Self::OffTarget,
        Self::AlreadyCustomer,
        Self::Competitor,
        Self::VendorOrIntegrator,
        Self::HostOrAgency,
        Self::BadData,
        Self::Other,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectionProblem {
    pub field: &'static str,
    pub message: String,
}

impl RejectionProblem {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    reason: RejectionReason,
    note: Option<String>,
}

impl Rejection {
    pub fn new(reason: RejectionReason, note: Option<&str>) -> Result<Self, RejectionProblem> {
        let note = note.map(str::trim).filter(|note| !note.is_empty());
        if note.is_some_and(|note| note.chars().count() > MAX_NOTE_CHARS) {
            return Err(RejectionProblem::new(
                "note",
                format!("must be at most {MAX_NOTE_CHARS} characters"),
            ));
        }
        if reason == RejectionReason::Other && note.is_none() {
            return Err(RejectionProblem::new(
                "note",
                "is required when the reason is autre",
            ));
        }
        Ok(Self {
            reason,
            note: note.map(str::to_owned),
        })
    }

    pub fn parse(reason: Option<&str>, note: Option<&str>) -> Result<Self, RejectionProblem> {
        let reason = reason
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .ok_or_else(|| RejectionProblem::new("reason", "is required to reject"))?;
        let reason = RejectionReason::parse(reason).ok_or_else(|| {
            let known: Vec<&str> = RejectionReason::ALL.iter().map(|r| r.as_str()).collect();
            RejectionProblem::new("reason", format!("must be one of {}", known.join(", ")))
        })?;
        Self::new(reason, note)
    }

    pub fn reason(&self) -> RejectionReason {
        self.reason
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_round_trips() {
        for reason in RejectionReason::ALL {
            assert_eq!(RejectionReason::parse(reason.as_str()), Some(reason));
        }
    }

    #[test]
    fn a_reason_is_required() {
        assert_eq!(Rejection::parse(None, None).unwrap_err().field, "reason");
        assert_eq!(
            Rejection::parse(Some("  "), None).unwrap_err().field,
            "reason"
        );
        assert_eq!(
            Rejection::parse(Some("pas_cible"), None).unwrap_err().field,
            "reason"
        );
    }

    #[test]
    fn the_note_is_trimmed_and_bounded() {
        let rejection = Rejection::parse(Some("hors_cible"), Some("  trop petit ")).unwrap();
        assert_eq!(rejection.reason(), RejectionReason::OffTarget);
        assert_eq!(rejection.note(), Some("trop petit"));
        assert_eq!(
            Rejection::parse(Some("hors_cible"), Some("   "))
                .unwrap()
                .note(),
            None
        );
        let long = "x".repeat(MAX_NOTE_CHARS + 1);
        assert_eq!(
            Rejection::parse(Some("hors_cible"), Some(&long))
                .unwrap_err()
                .field,
            "note"
        );
        let exact = "x".repeat(MAX_NOTE_CHARS);
        assert!(Rejection::parse(Some("autre"), Some(&exact)).is_ok());
    }

    #[test]
    fn the_free_text_is_required_for_autre() {
        assert_eq!(
            Rejection::parse(Some("autre"), None).unwrap_err().field,
            "note"
        );
        assert_eq!(
            Rejection::parse(Some("autre"), Some(" "))
                .unwrap_err()
                .field,
            "note"
        );
        assert!(Rejection::parse(Some("autre"), Some("raison")).is_ok());
    }
}
