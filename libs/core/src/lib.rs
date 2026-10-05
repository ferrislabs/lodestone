extern crate self as lodestone_core;

pub mod application;
pub(crate) mod domain;
pub mod infrastructure;

pub use application::*;
pub use domain::{
    NormalizedDomain, NormalizedDomainError, User, UserId,
    ats_target::AtsTargets,
    contact::{
        ContactFinder, ContactTargets, FoundAddress, rank_contacts, rank_of,
        recipient::{NotARecipient, Recipient},
        select_for_profile,
    },
    fetch::{
        FetchError, FetchPurpose, FetchRequest, FetchResponse,
        ports::{Fetcher, HostVisitRepository},
    },
    host::{
        CandidateHost, HostOrigin, Hostname, InvalidHostname,
        ports::{CandidateHosts, CtCursors, DnsError, DnsResolver, MxLookup},
    },
    job::{
        ClaimedJob, FailureDecision, InvalidJobKind, Job, JobId, JobKind, JobStatus, LeaseToken,
        NewJob, RetryPolicy, new_lease_token,
        ports::{Clock, JobFailure, JobQueue},
        schedule::{JobSchedule, ScheduledJob, SourceRunRecord, SourceSchedule},
    },
    lead::{
        Assessment, AssessmentId, ContactEmail, ContactId, ContactKind, ContactMethod, Evidence,
        InvalidScore, InvalidTransition, Lead, LeadId, LeadStatus, Score, Signal, SignalId,
        UnknownLeadStatus,
        draft::{
            DraftEdit, DraftEntry, DraftPage, DraftProblem, DraftQueue, DraftRecord, DraftReview,
            DraftStatus, InvalidDraftTransition, MAX_BODY_CHARS, MAX_REASON_CHARS,
            MAX_SUBJECT_CHARS, normalize_reason,
        },
        ports::{AssessmentRepository, ContactRepository, LeadRepository, SignalRepository},
        query::{LeadDetail, LeadListing, LeadPage, LeadQuery, LeadSort, ProfileSummary},
    },
    llm::{
        LlmError, LlmProvider, LlmRequest, LlmResponse, ProviderDefinition, ProviderKind,
        TokenUsage, UntrustedDocument,
        cache::{CacheScope, CachedOutput, LlmCache},
        provider::{
            CipherError, LlmProviderRecord, LlmProviderRepository, ProviderId, ResolvedProvider,
            SealedSecret, SecretCipher, Tier, TierError, TokenPricing,
        },
        render_user_content,
    },
    output::{
        AssessmentRecord, ContactRecord, DraftOutput, EvidenceRecord, LeadEvent, LeadEventKind,
        LeadRecord, LeadSink, ProfileRecord, SignalRecord, SinkError, WEBHOOK_JOB_KIND,
        WebhookJobPayload, WebhookPolicy, webhook_job,
    },
    partner::{InvalidSiret, Partner, PartnerAward, PartnerId, Siret, ports::PartnerRepository},
    profile::{
        ActiveProfile, Profile, ProfileId, ProfileName, ProfileNameError, ProfileProblem,
        ProfileRevision, ProfileValidationError, Revised, RevisionNumber, ValidatedProfile,
        commands::{CreateProfileCommand, ReviseProfileCommand},
        definition,
        ports::ProfileRepository,
    },
    prompt::{
        DraftMessage, EvidenceProblem, EvidenceQuote, PromptTemplate, QUALIFICATION_V1,
        QualificationOutput, TRIAGE_V1, TriageVerdict, market_context, market_context_with,
        verified_evidence,
    },
    run::{Run, RunId, RunOutcome, RunStatus, ports::RunRepository},
    scoring::{DISTINCT_SIGNALS_FACTOR, ScoreBreakdown, ScoreContribution, score},
    source::{Observation, Source, SourceError},
    suppression::{
        SuppressionEntry, SuppressionId, SuppressionTarget, ports::SuppressionRepository,
    },
    triage::{Candidate, Route, route},
    user::commands::CreateUserCommand,
    values::{EmailAddress, Provenance, SignalKind, SourceName, UnitInterval, ValueError},
};
