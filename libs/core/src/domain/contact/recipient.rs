use thiserror::Error;

use crate::domain::{
    lead::{ContactEmail, ContactMethod},
    values::EmailAddress,
};

#[derive(Debug, Error, PartialEq, Eq)]
#[error("a security.txt contact is a hint about who to reach, never a recipient")]
pub struct NotARecipient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient(EmailAddress);

impl Recipient {
    pub fn address(&self) -> &EmailAddress {
        &self.0
    }
}

impl TryFrom<&ContactEmail> for Recipient {
    type Error = NotARecipient;

    fn try_from(contact: &ContactEmail) -> Result<Self, Self::Error> {
        match contact.method {
            ContactMethod::SecurityTxt => Err(NotARecipient),
            ContactMethod::PublicPage | ContactMethod::RoleGuess | ContactMethod::LlmExtraction => {
                Ok(Self(contact.address.clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        lead::{ContactId, ContactKind, LeadId},
        values::{Provenance, UnitInterval},
    };

    fn contact(method: ContactMethod) -> ContactEmail {
        ContactEmail {
            id: ContactId(Uuid::nil()),
            lead_id: LeadId(Uuid::nil()),
            address: EmailAddress::parse("security@acme.com").unwrap(),
            provenance: Provenance::parse("https://acme.com/.well-known/security.txt").unwrap(),
            method,
            confidence: UnitInterval::new(0.7).unwrap(),
            kind: ContactKind::Role,
            found_at: Utc::now(),
        }
    }

    #[test]
    fn security_txt_contact_cannot_become_a_recipient() {
        assert_eq!(
            Recipient::try_from(&contact(ContactMethod::SecurityTxt)),
            Err(NotARecipient)
        );
    }

    #[test]
    fn other_contacts_can_become_recipients() {
        for method in [
            ContactMethod::PublicPage,
            ContactMethod::RoleGuess,
            ContactMethod::LlmExtraction,
        ] {
            let recipient = Recipient::try_from(&contact(method)).unwrap();
            assert_eq!(recipient.address().as_str(), "security@acme.com");
        }
    }
}
