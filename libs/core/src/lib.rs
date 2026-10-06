extern crate self as lodestone_core;

pub mod application;
pub(crate) mod domain;
pub mod infrastructure;

pub use application::evaluation::QUALIFICATION_LISTENER;
pub use application::*;
pub use domain::{
    NormalizedDomain, NormalizedDomainError, User, UserId,
    aggregation::{
        AggregationPrompt, AggregatorOutput, EVIDENCE_RETRY, Facts, build as aggregation_prompt,
        check as check_aggregation, instruction as aggregation_instruction,
        instruction_with_dossier as aggregation_instruction_with_dossier, missing_results,
        render_agent, retry_message as aggregation_retry_message, warnings as aggregation_warnings,
        with_problem as aggregation_problem,
    },
    ats_target::AtsTargets,
    company::{
        CompanyBuilder, CompanyDossier, CompanyPage, DossierBuild, DossierDocument, DossierState,
        InvalidSiren, OFFICIAL_SOURCE, OfficialData, PAGE_PROMPT_CHARS, PAGE_STORE_CHARS, Siren,
        bounded_error, headcount_label, naf_section_label, ports::CompanyRepository,
    },
    contact::{
        ContactFinder, ContactTargets, FoundAddress, rank_contacts, rank_of,
        recipient::{NotARecipient, Recipient},
        select_for_profile,
    },
    facts::{
        CompanyFacts, Country, CountryBasis, Environment, HostFact, LeadFacts, OfficialFacts,
        environment_of, signal_sentence,
    },
    fetch::{
        FetchError, FetchPurpose, FetchRequest, FetchResponse,
        ports::{Fetcher, HostVisitRepository},
    },
    host::{
        CandidateHost, HostOrigin, Hostname, InvalidHostname, TRANSIENT_RETRY,
        ports::{CandidateHosts, CtCursors, DnsError, DnsResolver, KnownHostRepository, MxLookup},
        transient_checked_at,
    },
    job::{
        ClaimedJob, FailureDecision, InvalidJobKind, Job, JobId, JobKind, JobStatus, LeaseToken,
        NewJob, RetryPolicy, new_lease_token,
        ports::{Clock, JobFailure, JobQueue},
        schedule::{JobSchedule, ScheduledJob, SourceRunRecord, SourceSchedule},
    },
    lead::{
        Assessment, AssessmentId, ContactEmail, ContactId, ContactKind, ContactMethod, Evidence,
        InvalidScore, InvalidTransition, Lead, LeadId, LeadStatus, QualityWarning, Score, Signal,
        SignalId, UnknownLeadStatus,
        draft::{
            DraftEdit, DraftEntry, DraftPage, DraftProblem, DraftQueue, DraftRecord, DraftReview,
            DraftStatus, InvalidDraftTransition, MAX_BODY_CHARS, MAX_REASON_CHARS,
            MAX_SUBJECT_CHARS, normalize_reason,
        },
        ports::{AssessmentRepository, ContactRepository, LeadRepository, SignalRepository},
        query::{
            LeadDetail, LeadListing, LeadPage, LeadQuery, LeadSort, ProfileSummary, VerdictFilter,
        },
    },
    listener::{
        CONTINUOUS_SOURCES, ListenerState, ListenerStatus, StatusUpdate, StepOutcome,
        is_continuous, lock_key, stale_after,
    },
    llm::{
        GenerationParams, GenerationProblem, GenerationSnapshot, LlmError, LlmProvider, LlmRequest,
        LlmResponse, MAX_TOKENS_LIMIT, ProviderDefinition, ProviderKind, ReasoningEffort,
        RunProvenance, TokenUsage, UntrustedDocument,
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
        contract::contract_schema,
        definition,
        ports::ProfileRepository,
    },
    prompt::{DraftMessage, EvidenceCheck, EvidenceQuote, check_evidence},
    qualification::{
        Answer, Completion, EvidenceComponents, EvidenceStrength, Exclusion, Judgement,
        LeadScoring, QualificationResult, ResultId, ResultStatus, SCORING_VERSION,
        SUMMARY_FAILED_AFTER, SummaryState, ValidAnswers, ValidOutput, Verdict, VerdictCap,
        VerdictReason, Violation, Violations, agent_score, calibrate, exclusions, judge,
        ports::QualificationRepository,
        prompt::{
            AgentPrompt, build as agent_prompt, lead_document, with_ungrounded, with_violations,
        },
        question_score, summary_retry_delay, validate_answers, verdict_for,
    },
    quality::{
        AgentSeparation, AgentSnapshot, ConfidenceBucket, DecisionCounts, DecisionKind,
        DecisionLabel, DecisionSnapshot, DiscardedShare, EXCLUSION_REASONS, ExclusionCandidate,
        ExclusionCount, ExclusionTarget, LabelFilter, MAX_EXCLUSION_CANDIDATES, MAX_NOTE_CHARS,
        MIN_LABELLED_LEADS, ProfileInfo, ProfileQuality, QualityReport, ReasonCount, Rejection,
        RejectionProblem, RejectionReason, Suggestion, VerdictCounts, VerdictPrecision,
        VersionPrecision, exclusion_targets, ports::DecisionRepository,
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
