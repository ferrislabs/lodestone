extern crate self as lodestone_core;

pub mod application;
pub(crate) mod domain;
pub mod infrastructure;

pub use application::*;
pub use domain::{
    NormalizedDomain, NormalizedDomainError, User, UserId,
    contact::{
        ContactFinder, ContactTargets, FoundAddress, rank_of,
        recipient::{NotARecipient, Recipient},
        select_for_profile,
    },
    fetch::{
        FetchError, FetchPurpose, FetchRequest, FetchResponse,
        ports::{Fetcher, HostVisitRepository},
    },
    host::{
        CandidateHost, HostOrigin, Hostname, InvalidHostname,
        ports::{CandidateHosts, CtCursors, DnsError, DnsResolver},
    },
    job::{
        ClaimedJob, FailureDecision, InvalidJobKind, Job, JobId, JobKind, JobStatus, LeaseToken,
        NewJob, RetryPolicy, new_lease_token,
        ports::{Clock, JobFailure, JobQueue},
    },
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
    source::{Observation, Source, SourceError},
    suppression::{
        SuppressionEntry, SuppressionId, SuppressionTarget, ports::SuppressionRepository,
    },
    user::commands::CreateUserCommand,
    values::{EmailAddress, Provenance, SignalKind, SourceName, UnitInterval, ValueError},
};
