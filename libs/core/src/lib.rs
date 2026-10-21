extern crate self as lodestone_core;

pub mod application;
pub(crate) mod domain;
pub mod infrastructure;

pub use application::evaluation::QUALIFICATION_LISTENER;
pub use application::*;
pub use domain::{
    NormalizedDomain, NormalizedDomainError, User, UserId,
    agent_work::{
        AGGREGATOR_AGENT_ID, AgentWork, CancelReason, ProfileAgents, WorkKind, WorkSnapshot,
        WorkTarget, all_cancelled, first_stop, kept_agents, only_removes_agents,
        ports::{AgentRegistry, AgentStop, AgentWorkRepository, KeptAgents},
        work_targets,
    },
    aggregation::{
        AggregationPrompt, AggregatorOutput, EVIDENCE_RETRY, Facts, build as aggregation_prompt,
        check as check_aggregation, instruction as aggregation_instruction,
        instruction_with_dossier as aggregation_instruction_with_dossier, missing_results,
        render_agent, render_scoring, retry_message as aggregation_retry_message, scoring_source,
        warnings as aggregation_warnings, with_problem as aggregation_problem,
    },
    ats_target::AtsTargets,
    calibration::{
        AgentCalibration, CALIBRATION_JOB_KIND, Calibration, CalibrationKind, CalibrationStatus,
        Calibrations, IsotonicMap, MIN_PER_CLASS, MIN_SAMPLES, Point, ProfileCalibration,
        ReliabilityBin, calibrate_verdict, ports::CalibrationRepository,
    },
    company::{
        CompanyBuilder, CompanyDossier, CompanyEntity, CompanyPage, DossierBuild, DossierDocument,
        DossierState, InvalidSiren, OFFICIAL_SOURCE, OfficialData, PAGE_PROMPT_CHARS,
        PAGE_STORE_CHARS, PageReview, PageVerifier, SiblingLead, Siren, bounded_error,
        headcount_label, naf_section_label, ports::CompanyRepository,
    },
    contact::{
        ContactFinder, ContactTargets, FoundAddress, rank_contacts, rank_of,
        recipient::{NotARecipient, Recipient},
        select_for_profile,
    },
    ecosystem::{
        DomainEcosystem, DueDomain, EcosystemFinding, EcosystemModule, EcosystemReadiness,
        FindingExample, HOSTING_PLATFORMS, Inspection, InspectionSettings, InspectionTarget,
        ModuleOutcome, ModuleRun, ModuleRunner, ModuleState, ModuleStats, ModuleStatus,
        PROVIDER_NOT_READY, SIREN_FINDING_KEY, SIREN_UNKNOWN, SirenLookup, ecosystem_documents,
        hosting_platform, ports::EcosystemRepository,
    },
    enrichment::{
        CERTIFICATE_SEEN_KIND, EnrichmentState, EnrichmentStatus, EnrichmentStep, InspectionRecord,
        ListedEnrichment, WaitingReason, enrichment_state, listed_enrichment_state,
        only_certificates,
    },
    facts::{
        CompanyFacts, Country, CountryBasis, Environment, HostFact, IndependentSignal, LeadFacts,
        OfficialFacts, SiblingSignals, environment_of, signal_sentence,
    },
    fetch::{
        FetchError, FetchPurpose, FetchRequest, FetchResponse,
        link::{LinkRejection, UnconfirmedLinks},
        ports::{Fetcher, HostVisitRepository},
    },
    host::{
        CandidateHost, HostOrigin, Hostname, InvalidHostname, TRANSIENT_RETRY,
        ports::{
            CandidateHosts, CtCursors, DnsError, DnsRecords, DnsResolver, KnownHostRepository,
            MxLookup,
        },
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
        contract::{contract_schema, scoring_contract_schema},
        definition,
        ports::ProfileRepository,
    },
    prompt::{DraftMessage, EvidenceCheck, EvidenceQuote, check_evidence},
    qualification::{
        AGENT_FAILED_AFTER, AgentFailure, AgentKind, Answer, Completion, EvidenceComponents,
        EvidenceStrength, Exclusion, Judgement, LeadScoring, QualificationResult, ResultId,
        ResultStatus, SCORING_VERSION, SUMMARY_FAILED_AFTER, SummaryState, VOTE_SHARE_WEIGHT,
        ValidAnswers, ValidOutput, Verdict, VerdictCap, VerdictReason, VerdictSource, Violation,
        Violations, agent_score, calibrate, exclusions, judge,
        ports::QualificationRepository,
        prompt::{
            AgentPrompt, build as agent_prompt, lead_document, with_ungrounded, with_violations,
        },
        question_score, sample_record, summary_retry_delay, validate_answers, verdict_for, vote,
    },
    quality::{
        AgentSeparation, AgentSnapshot, ConfidenceBucket, DecisionCounts, DecisionKind,
        DecisionLabel, DecisionSnapshot, DiscardedShare, EXCLUSION_REASONS, ExclusionCandidate,
        ExclusionCount, ExclusionTarget, LabelFilter, MAX_EXCLUSION_CANDIDATES, MAX_NOTE_CHARS,
        MIN_LABELLED_LEADS, ProfileInfo, ProfileQuality, QualityReport, ReasonCount, Rejection,
        RejectionProblem, RejectionReason, ScoringSnapshot, Suggestion, VerdictCounts,
        VerdictPrecision, VersionKind, VersionPrecision, exclusion_targets,
        ports::DecisionRepository,
    },
    run::{Run, RunId, RunOutcome, RunStatus, ports::RunRepository},
    scoring::{STRONGEST_SIGNALS, signal_strength, strength_score},
    scoring_agent::{
        AgentDecision, Decision, ScoringOutput, ScoringResult, ScoringResultId, ScoringViolation,
        ScoringViolations, VerdictAnswer, VerdictProbabilities, combine, decide,
        ports::{ScoringCompletion, ScoringRepository},
        prompt::{
            INDICATORS_SOURCE, REPORT_SOURCE_PREFIX, build as scoring_prompt, render_indicators,
            render_report, render_report_evidence, report_source,
            with_violations as scoring_violations,
        },
        round_score, scoring_sample_record, validate_scoring, vote_scoring,
    },
    source::{Observation, Source, SourceError},
    source_provider::{
        ATS, AuthKind, AuthSpec, BOAMP, BODACC, BODACC_DEFAULT_URL, CERTIFICATE_TRANSPARENCY,
        COMPANY_RESEARCH, CONTACTS, Capabilities, ConnectedAccount, Credential, DECP,
        DECP_HOLDERS_DEFAULT_URL, DNS_CNAME, FRANCE_TRAVAIL, FieldKind, FieldProblem, FieldSpec,
        FilterSupport, HACKER_NEWS_HIRING, HACKER_NEWS_THREAD_TITLE, HTTP_PROBE, OAUTH_STATE_TTL,
        OAuthState, ProbeContext, ProbeOutcome, ProviderCategory, ProviderConfiguration,
        ProviderProbe, ProviderStatus, SourceProviderDescriptor, SourceProviderRepository, TED,
        TOKEN_ACCESS, TOKEN_REFRESH, catalog, descriptor as provider_descriptor, mask_secret,
        provider_key_of_source, secret_context, validate_settings,
    },
    suppression::{
        SuppressionEntry, SuppressionId, SuppressionTarget, ports::SuppressionRepository,
    },
    triage::{Candidate, Route, countries::alpha3_of, passes_filters, route},
    user::commands::CreateUserCommand,
    values::{EmailAddress, Provenance, SignalKind, SourceName, UnitInterval, ValueError},
};
