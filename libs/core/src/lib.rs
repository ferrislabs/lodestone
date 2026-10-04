extern crate self as lodestone_core;

pub mod application;
pub(crate) mod domain;
pub mod infrastructure;

pub use application::*;
pub use domain::{
    NormalizedDomain, NormalizedDomainError, User, UserId,
    lead::{
        Assessment, AssessmentId, ContactEmail, ContactId, ContactKind, ContactMethod, Evidence,
        InvalidScore, InvalidTransition, Lead, LeadId, LeadStatus, Score, Signal, SignalId,
        UnknownLeadStatus,
        ports::{AssessmentRepository, ContactRepository, LeadRepository, SignalRepository},
    },
    profile::{
        ActiveProfile, Profile, ProfileId, ProfileName, ProfileNameError, ProfileProblem,
        ProfileRevision, ProfileValidationError, RevisionNumber, ValidatedProfile,
        commands::{CreateProfileCommand, ReviseProfileCommand},
        definition,
        ports::ProfileRepository,
    },
    run::{Run, RunId, RunOutcome, RunStatus, ports::RunRepository},
    suppression::{
        SuppressionEntry, SuppressionId, SuppressionTarget, ports::SuppressionRepository,
    },
    user::commands::CreateUserCommand,
    values::{EmailAddress, Provenance, SignalKind, SourceName, UnitInterval, ValueError},
};
