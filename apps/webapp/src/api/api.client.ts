export namespace Schemas {
  // <Schemas>
  export type AgentQualityDto = {
    agent_id: string;
    mean_rejected?: (number | null) | undefined;
    mean_validated?: (number | null) | undefined;
    rejected: number;
    separation?: (number | null) | undefined;
    validated: number;
  };
  export type DraftStatusDto = "pending" | "approved" | "rejected";
  export type DraftDto = {
    body: string;
    edited_at?: (string | null) | undefined;
    original_body?: (string | null) | undefined;
    original_subject?: (string | null) | undefined;
    outdated_at?: (string | null) | undefined;
    rejection_reason?: (string | null) | undefined;
    reviewed_at?: (string | null) | undefined;
    reviewed_by?: (string | null) | undefined;
    status: DraftStatusDto;
    subject: string;
    suggested_body?: (string | null) | undefined;
    suggested_subject?: (string | null) | undefined;
  };
  export type EvidenceDto = { provenance: string; quote: string };
  export type AssessmentDto = {
    approach_angle: string;
    created_at: string;
    draft?: (null | DraftDto) | undefined;
    evidence: Array<EvidenceDto>;
    evidence_verified: boolean;
    id: string;
    model: string;
    profile_revision: number;
    prompt_version: string;
    quality_warnings: Array<string>;
    summary: string;
    verdict_narrative: string;
  };
  export type DraftOutput = { body: string; status: string; subject: string };
  export type EvidenceRecord = { provenance: string; quote: string };
  export type AssessmentRecord = {
    approach_angle: string;
    draft?: (null | DraftOutput) | undefined;
    evidence: Array<EvidenceRecord>;
    summary: string;
  };
  export type AssignTierRequest = { provider_id: string };
  export type LeadStatusDto = "new" | "qualified" | "validated" | "rejected" | "contacted" | "uncertain";
  export type ChangeLeadStatusRequest = { status: LeadStatusDto };
  export type OfficialDataDto = {
    annuaire_url: string;
    created_on?: (string | null) | undefined;
    head_office_city?: (string | null) | undefined;
    head_office_country?: (string | null) | undefined;
    head_office_postcode?: (string | null) | undefined;
    headcount_range?: (string | null) | undefined;
    legal_name: string;
    naf_code?: (string | null) | undefined;
    naf_label?: (string | null) | undefined;
    siren: string;
  };
  export type CompanyPageDto = { url: string };
  export type CompanyDto = {
    built_at?: (string | null) | undefined;
    error?: (string | null) | undefined;
    next_attempt_at: string;
    official?: (null | OfficialDataDto) | undefined;
    pages: Array<CompanyPageDto>;
    siren?: (string | null) | undefined;
    state: string;
  };
  export type ConfidenceBucketDto = { from: number; rejected: number; to: number; validated: number };
  export type ContactDto = {
    address: string;
    confidence: number;
    found_at: string;
    id: string;
    kind: string;
    method: string;
    provenance: string;
  };
  export type ContactRecord = { address: string; confidence: number; method: string; provenance: string };
  export type CountryDto = { code: string; name: string };
  export type CreateProfileRequest = { definition: Record<string, unknown>; name?: (string | null) | undefined };
  export type PricingRequest = Partial<{ input_cents_per_mtok: number | null; output_cents_per_mtok: number | null }>;
  export type ReasoningEffort = "none" | "low" | "medium" | "high";
  export type CreateProviderRequest = {
    api_key?: (string | null) | undefined;
    base_url: string;
    frequency_penalty?: (number | null) | undefined;
    kind: string;
    max_retries?: (number | null) | undefined;
    max_tokens?: (number | null) | undefined;
    model: string;
    name: string;
    presence_penalty?: (number | null) | undefined;
    pricing?: (null | PricingRequest) | undefined;
    reasoning_effort?: (null | ReasoningEffort) | undefined;
    repeat_penalty?: (number | null) | undefined;
    temperature?: (number | null) | undefined;
    timeout_seconds?: (number | null) | undefined;
  };
  export type PaginationMetadata = {
    current_page: number;
    first_page: number;
    is_empty: boolean;
    last_page?: (number | null) | undefined;
    next_page?: (number | null) | undefined;
    per_page: number;
    prev_page?: (number | null) | undefined;
    total?: (number | null) | undefined;
  };
  export type DataEnvelope_CompanyDto = {
    data: {
      built_at?: (string | null) | undefined;
      error?: (string | null) | undefined;
      next_attempt_at: string;
      official?: (null | OfficialDataDto) | undefined;
      pages: Array<CompanyPageDto>;
      siren?: (string | null) | undefined;
      state: string;
    };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DraftContactDto = { address: string; confidence: number; method: string };
  export type ProfileRefDto = { id: string; name: string };
  export type VerdictDto = "fiable" | "a_verifier" | "ecarte";
  export type DraftLeadDto = {
    confidence?: (number | null) | undefined;
    domain: string;
    id: string;
    profile: ProfileRefDto;
    score: number;
    verdict?: (null | VerdictDto) | undefined;
  };
  export type DataEnvelope_DraftEntryDto = {
    data: {
      approach_angle: string;
      assessment_id: string;
      contact?: (null | DraftContactDto) | undefined;
      created_at: string;
      draft: DraftDto;
      evidence: Array<EvidenceDto>;
      evidence_verified: boolean;
      lead: DraftLeadDto;
      summary: string;
      verdict_narrative: string;
    };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type HostFactDto = { environment: string; host: string; signals: number };
  export type OfficialFactsDto = {
    head_office_country?: (string | null) | undefined;
    headcount_range?: (string | null) | undefined;
    legal_name: string;
    naf_code?: (string | null) | undefined;
    naf_label?: (string | null) | undefined;
    siren: string;
  };
  export type LeadFactsDto = {
    country_basis?: (string | null) | undefined;
    distinct_hosts: number;
    distinct_subdomains: number;
    first_seen_at: string;
    hosts: Array<HostFactDto>;
    last_seen_at: string;
    official?: (null | OfficialFactsDto) | undefined;
    only_non_production: boolean;
    probable_country?: (null | CountryDto) | undefined;
    production_hosts: number;
    production_share?: (number | null) | undefined;
    signal_count: number;
    source_count: number;
    sources: Array<string>;
  };
  export type EvidenceComponentsDto = { diversity: number; environment: number; strength: number; volume: number };
  export type EvidenceStrengthDto = {
    components: EvidenceComponentsDto;
    only_non_production: boolean;
    signal_count: number;
    signal_score: number;
    strength: number;
  };
  export type VerdictReasonDto = "single_angle" | "weak_evidence" | "non_production";
  export type LeadDto = {
    confidence?: (number | null) | undefined;
    domain: string;
    evidence?: (null | EvidenceStrengthDto) | undefined;
    first_seen_at: string;
    id: string;
    last_seen_at: string;
    opinion?: (number | null) | undefined;
    profile: ProfileRefDto;
    qualified_at?: (string | null) | undefined;
    score: number;
    status: LeadStatusDto;
    summary?: (string | null) | undefined;
    updated_at: string;
    verdict?: (null | VerdictDto) | undefined;
    verdict_reason?: (null | VerdictReasonDto) | undefined;
  };
  export type ProbabilityDto = { label: string; probability: number; value: string };
  export type QualificationAnswerDto = {
    choice: string;
    choice_label: string;
    confidence: number;
    probabilities: Array<ProbabilityDto>;
    question_id: string;
    wording: string;
  };
  export type QualificationResultDto = {
    agent_description?: (string | null) | undefined;
    agent_id: string;
    agent_name: string;
    agent_score?: (number | null) | undefined;
    answers: Array<QualificationAnswerDto>;
    created_at: string;
    error?: (string | null) | undefined;
    model: string;
    provider_id?: (string | null) | undefined;
    provider_name?: (string | null) | undefined;
    status: string;
    summary?: (string | null) | undefined;
  };
  export type ScoreContributionDto = {
    contribution: number;
    measure: number;
    signal: string;
    value?: (string | null) | undefined;
    weight: number;
  };
  export type SignalDto = {
    agent?: (string | null) | undefined;
    id: string;
    kind: string;
    observed_at: string;
    provenance: string;
    raw: Record<string, unknown>;
    sentence: string;
    source: string;
    strength: number;
    value?: (string | null) | undefined;
  };
  export type SummaryStateDto = {
    attempts: number;
    error?: (string | null) | undefined;
    failed: boolean;
    next_attempt_at?: (string | null) | undefined;
  };
  export type DataEnvelope_LeadDetailDto = {
    data: {
      assessments: Array<AssessmentDto>;
      company?: (null | CompanyDto) | undefined;
      contacts: Array<ContactDto>;
      facts: LeadFactsDto;
      lead: LeadDto;
      qualification_results: Array<QualificationResultDto>;
      score_breakdown: Array<ScoreContributionDto>;
      signals: Array<SignalDto>;
      summary_state?: (null | SummaryStateDto) | undefined;
    };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_LeadDto = {
    data: {
      confidence?: (number | null) | undefined;
      domain: string;
      evidence?: (null | EvidenceStrengthDto) | undefined;
      first_seen_at: string;
      id: string;
      last_seen_at: string;
      opinion?: (number | null) | undefined;
      profile: ProfileRefDto;
      qualified_at?: (string | null) | undefined;
      score: number;
      status: LeadStatusDto;
      summary?: (string | null) | undefined;
      updated_at: string;
      verdict?: (null | VerdictDto) | undefined;
      verdict_reason?: (null | VerdictReasonDto) | undefined;
    };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type UserId = string;
  export type DataEnvelope_MeResponse = {
    data: { kind: "user"; subject: string; user_id: UserId } | { client_id: string; kind: "client" };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type ProfileDto = { active: boolean; created_at: string; id: string; name: string; updated_at: string };
  export type DataEnvelope_ProfileDetailDto = {
    data: ProfileDto & { definition: Record<string, unknown>; revision: number };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_ProfileDto = {
    data: { active: boolean; created_at: string; id: string; name: string; updated_at: string };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type PricingDto = { input_cents_per_mtok: number; output_cents_per_mtok: number };
  export type DataEnvelope_ProviderDto = {
    data: {
      base_url: string;
      created_at: string;
      enabled: boolean;
      frequency_penalty?: (number | null) | undefined;
      has_api_key: boolean;
      id: string;
      kind: string;
      max_retries: number;
      max_tokens?: (number | null) | undefined;
      model: string;
      name: string;
      presence_penalty?: (number | null) | undefined;
      pricing: PricingDto;
      reasoning_effort?: (null | ReasoningEffort) | undefined;
      repeat_penalty?: (number | null) | undefined;
      temperature?: (number | null) | undefined;
      tiers: Array<string>;
      timeout_seconds: number;
      updated_at: string;
    };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DecisionCountsDto = {
    drafts_approved: number;
    drafts_rejected: number;
    rejected: number;
    validated: number;
  };
  export type DiscardedShareDto = { discarded: number; share?: (number | null) | undefined; total: number };
  export type ThresholdsDto = { discard_below: number; reliable_from: number };
  export type SuggestionDto = {
    discard_below?: (number | null) | undefined;
    labelled: number;
    ready: boolean;
    reliable_from?: (number | null) | undefined;
    required: number;
  };
  export type ProfileQualityDto = {
    current: ThresholdsDto;
    name: string;
    precision?: (number | null) | undefined;
    profile_id: string;
    rejected: number;
    suggestion: SuggestionDto;
    validated: number;
  };
  export type VerdictPrecisionDto = {
    n: number;
    precision?: (number | null) | undefined;
    rejected: number;
    validated: number;
    verdict?: (null | VerdictDto) | undefined;
  };
  export type DataEnvelope_QualityDto = {
    data: {
      agents: Array<AgentQualityDto>;
      confidence: Array<ConfidenceBucketDto>;
      decisions: DecisionCountsDto;
      discarded: DiscardedShareDto;
      profiles: Array<ProfileQualityDto>;
      verdicts: Array<VerdictPrecisionDto>;
    };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type RevisionSummaryDto = { created_at: string; created_by?: (string | null) | undefined; number: number };
  export type DataEnvelope_RevisionDto = {
    data: RevisionSummaryDto & { definition: Record<string, unknown> };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type SourceRunDto = {
    agent_id?: (string | null) | undefined;
    error?: (string | null) | undefined;
    finished_at?: (string | null) | undefined;
    profile_id: string;
    profile_name: string;
    signals: number;
    started_at: string;
    status: string;
    volume: number;
  };
  export type ListenerDto = {
    backlog?: (number | null) | undefined;
    holder?: (string | null) | undefined;
    items_total: number;
    last_activity_at?: (string | null) | undefined;
    last_error?: (string | null) | undefined;
    signals_total: number;
    state: string;
    updated_at?: (string | null) | undefined;
  };
  export type DataEnvelope_SourceDto = {
    data: {
      attempts: number;
      kind: string;
      label_key: string;
      last_error?: (string | null) | undefined;
      last_runs: Array<SourceRunDto>;
      lease_until?: (string | null) | undefined;
      listener?: (null | ListenerDto) | undefined;
      mode: string;
      next_run_at?: (string | null) | undefined;
      running_since?: (string | null) | undefined;
      source: string;
      status: string;
      unavailable_reason?: (string | null) | undefined;
    };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type TestErrorDto = { kind: string; message: string };
  export type DataEnvelope_TestResultDto = {
    data: {
      error?: (null | TestErrorDto) | undefined;
      latency_ms?: (number | null) | undefined;
      model?: (string | null) | undefined;
      ok: boolean;
    };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_TierDto = {
    data: { provider_id?: (string | null) | undefined; tier: string };
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_Vec_DraftEntryDto = {
    data: Array<{
      approach_angle: string;
      assessment_id: string;
      contact?: (null | DraftContactDto) | undefined;
      created_at: string;
      draft: DraftDto;
      evidence: Array<EvidenceDto>;
      evidence_verified: boolean;
      lead: DraftLeadDto;
      summary: string;
      verdict_narrative: string;
    }>;
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_Vec_LeadDto = {
    data: Array<{
      confidence?: (number | null) | undefined;
      domain: string;
      evidence?: (null | EvidenceStrengthDto) | undefined;
      first_seen_at: string;
      id: string;
      last_seen_at: string;
      opinion?: (number | null) | undefined;
      profile: ProfileRefDto;
      qualified_at?: (string | null) | undefined;
      score: number;
      status: LeadStatusDto;
      summary?: (string | null) | undefined;
      updated_at: string;
      verdict?: (null | VerdictDto) | undefined;
      verdict_reason?: (null | VerdictReasonDto) | undefined;
    }>;
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_Vec_ProfileSummaryDto = {
    data: Array<{
      active: boolean;
      id: string;
      lead_count: number;
      name: string;
      revision?: (number | null) | undefined;
    }>;
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_Vec_ProviderDto = {
    data: Array<{
      base_url: string;
      created_at: string;
      enabled: boolean;
      frequency_penalty?: (number | null) | undefined;
      has_api_key: boolean;
      id: string;
      kind: string;
      max_retries: number;
      max_tokens?: (number | null) | undefined;
      model: string;
      name: string;
      presence_penalty?: (number | null) | undefined;
      pricing: PricingDto;
      reasoning_effort?: (null | ReasoningEffort) | undefined;
      repeat_penalty?: (number | null) | undefined;
      temperature?: (number | null) | undefined;
      tiers: Array<string>;
      timeout_seconds: number;
      updated_at: string;
    }>;
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_Vec_RevisionSummaryDto = {
    data: Array<{ created_at: string; created_by?: (string | null) | undefined; number: number }>;
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_Vec_SourceDto = {
    data: Array<{
      attempts: number;
      kind: string;
      label_key: string;
      last_error?: (string | null) | undefined;
      last_runs: Array<SourceRunDto>;
      lease_until?: (string | null) | undefined;
      listener?: (null | ListenerDto) | undefined;
      mode: string;
      next_run_at?: (string | null) | undefined;
      running_since?: (string | null) | undefined;
      source: string;
      status: string;
      unavailable_reason?: (string | null) | undefined;
    }>;
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DataEnvelope_Vec_TierDto = {
    data: Array<{ provider_id?: (string | null) | undefined; tier: string }>;
    pagination?: (null | PaginationMetadata) | undefined;
  };
  export type DraftEntryDto = {
    approach_angle: string;
    assessment_id: string;
    contact?: (null | DraftContactDto) | undefined;
    created_at: string;
    draft: DraftDto;
    evidence: Array<EvidenceDto>;
    evidence_verified: boolean;
    lead: DraftLeadDto;
    summary: string;
    verdict_narrative: string;
  };
  export type EditDraftRequest = Partial<{ body: string | null; subject: string | null }>;
  export type ErrorBody = {
    code: string;
    details?: (Array<ErrorDetail> | null) | undefined;
    message: string;
    status: number;
  };
  export type ErrorDetail = { message: string; path: string };
  export type LeadDetailDto = {
    assessments: Array<AssessmentDto>;
    company?: (null | CompanyDto) | undefined;
    contacts: Array<ContactDto>;
    facts: LeadFactsDto;
    lead: LeadDto;
    qualification_results: Array<QualificationResultDto>;
    score_breakdown: Array<ScoreContributionDto>;
    signals: Array<SignalDto>;
    summary_state?: (null | SummaryStateDto) | undefined;
  };
  export type ProfileRecord = { id: string; name: string; revision?: (number | null) | undefined };
  export type SignalRecord = {
    kind: string;
    observed_at: string;
    provenance: string;
    source: string;
    value?: (string | null) | undefined;
  };
  export type LeadRecord = {
    assessment?: (null | AssessmentRecord) | undefined;
    contact?: (null | ContactRecord) | undefined;
    domain: string;
    first_seen_at: string;
    last_seen_at: string;
    lead_id: string;
    profile: ProfileRecord;
    score: number;
    score_breakdown: Array<Record<string, unknown>>;
    signals: Array<SignalRecord>;
    status: string;
  };
  export type MeResponse = { kind: "user"; subject: string; user_id: UserId } | { client_id: string; kind: "client" };
  export type ProfileDetailDto = ProfileDto & { definition: Record<string, unknown>; revision: number };
  export type ProfileSummaryDto = {
    active: boolean;
    id: string;
    lead_count: number;
    name: string;
    revision?: (number | null) | undefined;
  };
  export type ProviderDto = {
    base_url: string;
    created_at: string;
    enabled: boolean;
    frequency_penalty?: (number | null) | undefined;
    has_api_key: boolean;
    id: string;
    kind: string;
    max_retries: number;
    max_tokens?: (number | null) | undefined;
    model: string;
    name: string;
    presence_penalty?: (number | null) | undefined;
    pricing: PricingDto;
    reasoning_effort?: (null | ReasoningEffort) | undefined;
    repeat_penalty?: (number | null) | undefined;
    temperature?: (number | null) | undefined;
    tiers: Array<string>;
    timeout_seconds: number;
    updated_at: string;
  };
  export type QualityDto = {
    agents: Array<AgentQualityDto>;
    confidence: Array<ConfidenceBucketDto>;
    decisions: DecisionCountsDto;
    discarded: DiscardedShareDto;
    profiles: Array<ProfileQualityDto>;
    verdicts: Array<VerdictPrecisionDto>;
  };
  export type RejectDraftRequest = Partial<{ reason: string | null }>;
  export type RevisionDto = RevisionSummaryDto & { definition: Record<string, unknown> };
  export type SourceDto = {
    attempts: number;
    kind: string;
    label_key: string;
    last_error?: (string | null) | undefined;
    last_runs: Array<SourceRunDto>;
    lease_until?: (string | null) | undefined;
    listener?: (null | ListenerDto) | undefined;
    mode: string;
    next_run_at?: (string | null) | undefined;
    running_since?: (string | null) | undefined;
    source: string;
    status: string;
    unavailable_reason?: (string | null) | undefined;
  };
  export type TestResultDto = {
    error?: (null | TestErrorDto) | undefined;
    latency_ms?: (number | null) | undefined;
    model?: (string | null) | undefined;
    ok: boolean;
  };
  export type TierDto = { provider_id?: (string | null) | undefined; tier: string };
  export type UpdateProfileRequest = { definition: Record<string, unknown> };
  export type UpdateProviderRequest = Partial<{
    api_key: string | null;
    base_url: string | null;
    frequency_penalty: number | null;
    kind: string | null;
    max_retries: number | null;
    max_tokens: number | null;
    model: string | null;
    name: string | null;
    presence_penalty: number | null;
    pricing: null | PricingRequest;
    reasoning_effort: null | ReasoningEffort;
    repeat_penalty: number | null;
    temperature: number | null;
    timeout_seconds: number | null;
  }>;

  // </Schemas>
}

export namespace Endpoints {
  // <Endpoints>

  export type get_List_drafts = {
    method: "GET";
    path: "/api/v1/drafts";
    requestFormat: "json";
    parameters: {
      query: Partial<{ profile_id: string; verdict: string; page: number; per_page: number }>;
    };
    responses: { 200: Schemas.DataEnvelope_Vec_DraftEntryDto; 400: unknown; 401: unknown; 404: unknown };
  };
  export type get_Get_draft = {
    method: "GET";
    path: "/api/v1/drafts/{assessment_id}";
    requestFormat: "json";
    parameters: {
      path: { assessment_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_DraftEntryDto; 400: unknown; 401: unknown; 404: unknown };
  };
  export type patch_Edit_draft = {
    method: "PATCH";
    path: "/api/v1/drafts/{assessment_id}";
    requestFormat: "json";
    parameters: {
      path: { assessment_id: string };

      body: Schemas.EditDraftRequest;
    };
    responses: {
      200: Schemas.DataEnvelope_DraftEntryDto;
      400: unknown;
      401: unknown;
      403: unknown;
      404: unknown;
      409: unknown;
      422: unknown;
    };
  };
  export type post_Approve_draft = {
    method: "POST";
    path: "/api/v1/drafts/{assessment_id}/approve";
    requestFormat: "json";
    parameters: {
      path: { assessment_id: string };
    };
    responses: {
      200: Schemas.DataEnvelope_DraftEntryDto;
      400: unknown;
      401: unknown;
      403: unknown;
      404: unknown;
      409: unknown;
    };
  };
  export type post_Reject_draft = {
    method: "POST";
    path: "/api/v1/drafts/{assessment_id}/reject";
    requestFormat: "json";
    parameters: {
      path: { assessment_id: string };

      body: Schemas.RejectDraftRequest;
    };
    responses: {
      200: Schemas.DataEnvelope_DraftEntryDto;
      400: unknown;
      401: unknown;
      403: unknown;
      404: unknown;
      409: unknown;
      422: unknown;
    };
  };
  export type get_Lead_detail = {
    method: "GET";
    path: "/api/v1/leads/{lead_id}";
    requestFormat: "json";
    parameters: {
      path: { lead_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_LeadDetailDto; 400: unknown; 401: unknown; 404: unknown };
  };
  export type post_Refresh_company = {
    method: "POST";
    path: "/api/v1/leads/{lead_id}/company/refresh";
    requestFormat: "json";
    parameters: {
      path: { lead_id: string };
    };
    responses: { 202: Schemas.DataEnvelope_CompanyDto; 400: unknown; 401: unknown; 403: unknown; 404: unknown };
  };
  export type patch_Change_lead_status = {
    method: "PATCH";
    path: "/api/v1/leads/{lead_id}/status";
    requestFormat: "json";
    parameters: {
      path: { lead_id: string };

      body: Schemas.ChangeLeadStatusRequest;
    };
    responses: {
      200: Schemas.DataEnvelope_LeadDto;
      400: unknown;
      401: unknown;
      403: unknown;
      404: unknown;
      409: unknown;
    };
  };
  export type post_Retry_lead_summary = {
    method: "POST";
    path: "/api/v1/leads/{lead_id}/summary/retry";
    requestFormat: "json";
    parameters: {
      path: { lead_id: string };
    };
    responses: {
      200: Schemas.DataEnvelope_LeadDto;
      400: unknown;
      401: unknown;
      403: unknown;
      404: unknown;
      409: unknown;
    };
  };
  export type get_List_providers = {
    method: "GET";
    path: "/api/v1/llm/providers";
    requestFormat: "json";
    parameters: never;
    responses: { 200: Schemas.DataEnvelope_Vec_ProviderDto; 401: unknown };
  };
  export type post_Create_provider = {
    method: "POST";
    path: "/api/v1/llm/providers";
    requestFormat: "json";
    parameters: {
      body: Schemas.CreateProviderRequest;
    };
    responses: {
      201: Schemas.DataEnvelope_ProviderDto;
      401: unknown;
      403: unknown;
      409: unknown;
      422: Schemas.ErrorBody;
    };
  };
  export type get_Get_provider = {
    method: "GET";
    path: "/api/v1/llm/providers/{provider_id}";
    requestFormat: "json";
    parameters: {
      path: { provider_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_ProviderDto; 400: unknown; 401: unknown; 404: unknown };
  };
  export type delete_Delete_provider = {
    method: "DELETE";
    path: "/api/v1/llm/providers/{provider_id}";
    requestFormat: "json";
    parameters: {
      path: { provider_id: string };
    };
    responses: { 204: unknown; 400: unknown; 401: unknown; 403: unknown; 404: unknown; 409: Schemas.ErrorBody };
  };
  export type patch_Update_provider = {
    method: "PATCH";
    path: "/api/v1/llm/providers/{provider_id}";
    requestFormat: "json";
    parameters: {
      path: { provider_id: string };

      body: Schemas.UpdateProviderRequest;
    };
    responses: {
      200: Schemas.DataEnvelope_ProviderDto;
      400: unknown;
      401: unknown;
      403: unknown;
      404: unknown;
      409: unknown;
      422: Schemas.ErrorBody;
    };
  };
  export type post_Disable_provider = {
    method: "POST";
    path: "/api/v1/llm/providers/{provider_id}/disable";
    requestFormat: "json";
    parameters: {
      path: { provider_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_ProviderDto; 400: unknown; 401: unknown; 403: unknown; 404: unknown };
  };
  export type post_Enable_provider = {
    method: "POST";
    path: "/api/v1/llm/providers/{provider_id}/enable";
    requestFormat: "json";
    parameters: {
      path: { provider_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_ProviderDto; 400: unknown; 401: unknown; 403: unknown; 404: unknown };
  };
  export type post_Test_provider = {
    method: "POST";
    path: "/api/v1/llm/providers/{provider_id}/test";
    requestFormat: "json";
    parameters: {
      path: { provider_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_TestResultDto; 400: unknown; 401: unknown; 403: unknown; 404: unknown };
  };
  export type get_List_tiers = {
    method: "GET";
    path: "/api/v1/llm/tiers";
    requestFormat: "json";
    parameters: never;
    responses: { 200: Schemas.DataEnvelope_Vec_TierDto; 401: unknown };
  };
  export type put_Assign_tier = {
    method: "PUT";
    path: "/api/v1/llm/tiers/{tier}";
    requestFormat: "json";
    parameters: {
      path: { tier: string };

      body: Schemas.AssignTierRequest;
    };
    responses: { 200: Schemas.DataEnvelope_TierDto; 400: unknown; 401: unknown; 403: unknown; 422: Schemas.ErrorBody };
  };
  export type delete_Unassign_tier = {
    method: "DELETE";
    path: "/api/v1/llm/tiers/{tier}";
    requestFormat: "json";
    parameters: {
      path: { tier: string };
    };
    responses: { 204: unknown; 400: unknown; 401: unknown; 403: unknown; 404: unknown };
  };
  export type get_Me = {
    method: "GET";
    path: "/api/v1/me";
    requestFormat: "json";
    parameters: never;
    responses: { 200: Schemas.DataEnvelope_MeResponse; 401: unknown };
  };
  export type get_List_profiles = {
    method: "GET";
    path: "/api/v1/profiles";
    requestFormat: "json";
    parameters: never;
    responses: { 200: Schemas.DataEnvelope_Vec_ProfileSummaryDto; 401: unknown };
  };
  export type post_Create_profile = {
    method: "POST";
    path: "/api/v1/profiles";
    requestFormat: "json";
    parameters: {
      body: Schemas.CreateProfileRequest;
    };
    responses: {
      201: Schemas.DataEnvelope_ProfileDetailDto;
      401: unknown;
      403: unknown;
      409: unknown;
      422: Schemas.ErrorBody;
    };
  };
  export type post_Import_profile = {
    method: "POST";
    path: "/api/v1/profiles/import";
    requestFormat: "json";
    parameters: {
      query: Partial<{ name: string }>;

      body: Record<string, unknown>;
    };
    responses: {
      201: Schemas.DataEnvelope_ProfileDetailDto;
      401: unknown;
      403: unknown;
      409: unknown;
      422: Schemas.ErrorBody;
    };
  };
  export type get_Get_profile = {
    method: "GET";
    path: "/api/v1/profiles/{profile_id}";
    requestFormat: "json";
    parameters: {
      path: { profile_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_ProfileDetailDto; 400: unknown; 401: unknown; 404: unknown };
  };
  export type put_Update_profile = {
    method: "PUT";
    path: "/api/v1/profiles/{profile_id}";
    requestFormat: "json";
    parameters: {
      path: { profile_id: string };

      body: Schemas.UpdateProfileRequest;
    };
    responses: {
      200: Schemas.DataEnvelope_RevisionDto;
      201: Schemas.DataEnvelope_RevisionDto;
      400: unknown;
      401: unknown;
      403: unknown;
      404: unknown;
      422: Schemas.ErrorBody;
    };
  };
  export type post_Activate_profile = {
    method: "POST";
    path: "/api/v1/profiles/{profile_id}/activate";
    requestFormat: "json";
    parameters: {
      path: { profile_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_ProfileDto; 400: unknown; 401: unknown; 403: unknown; 404: unknown };
  };
  export type post_Deactivate_profile = {
    method: "POST";
    path: "/api/v1/profiles/{profile_id}/deactivate";
    requestFormat: "json";
    parameters: {
      path: { profile_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_ProfileDto; 400: unknown; 401: unknown; 403: unknown; 404: unknown };
  };
  export type get_Export_profile = {
    method: "GET";
    path: "/api/v1/profiles/{profile_id}/export";
    requestFormat: "json";
    parameters: {
      path: { profile_id: string };
    };
    responses: { 200: Record<string, unknown>; 400: unknown; 401: unknown; 404: unknown };
  };
  export type get_List_leads = {
    method: "GET";
    path: "/api/v1/profiles/{profile_id}/leads";
    requestFormat: "json";
    parameters: {
      query: Partial<{
        status: "new" | "qualified" | "validated" | "rejected" | "contacted" | "uncertain";
        verdict: string;
        min_confidence: number;
        min_score: number;
        domain: string;
        signal: string;
        sort: string;
        page: number;
        per_page: number;
      }>;
      path: { profile_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_Vec_LeadDto; 400: unknown; 401: unknown; 404: unknown };
  };
  export type get_Export_leads = {
    method: "GET";
    path: "/api/v1/profiles/{profile_id}/leads/export";
    requestFormat: "json";
    parameters: {
      query: Partial<{
        format: string;
        status: string;
        verdict: string;
        min_confidence: number;
        min_score: number;
        domain: string;
        signal: string;
        sort: string;
      }>;
      path: { profile_id: string };
    };
    responses: { 200: Array<Schemas.LeadRecord>; 400: unknown; 401: unknown; 404: unknown; 422: unknown };
  };
  export type get_List_revisions = {
    method: "GET";
    path: "/api/v1/profiles/{profile_id}/revisions";
    requestFormat: "json";
    parameters: {
      path: { profile_id: string };
    };
    responses: { 200: Schemas.DataEnvelope_Vec_RevisionSummaryDto; 400: unknown; 401: unknown; 404: unknown };
  };
  export type get_Get_revision = {
    method: "GET";
    path: "/api/v1/profiles/{profile_id}/revisions/{number}";
    requestFormat: "json";
    parameters: {
      path: { profile_id: string; number: number };
    };
    responses: { 200: Schemas.DataEnvelope_RevisionDto; 400: unknown; 401: unknown; 404: unknown };
  };
  export type get_Quality = {
    method: "GET";
    path: "/api/v1/quality";
    requestFormat: "json";
    parameters: {
      query: Partial<{ from: string; to: string; profile_id: string }>;
    };
    responses: { 200: Schemas.DataEnvelope_QualityDto; 400: unknown; 401: unknown; 403: unknown; 404: unknown };
  };
  export type get_List_sources = {
    method: "GET";
    path: "/api/v1/sources";
    requestFormat: "json";
    parameters: never;
    responses: { 200: Schemas.DataEnvelope_Vec_SourceDto; 401: unknown };
  };
  export type post_Run_source = {
    method: "POST";
    path: "/api/v1/sources/{source}/run";
    requestFormat: "json";
    parameters: {
      path: { source: string };
    };
    responses: {
      202: Schemas.DataEnvelope_SourceDto;
      400: unknown;
      401: unknown;
      403: unknown;
      404: unknown;
      409: unknown;
    };
  };

  // </Endpoints>
}

// <EndpointByMethod>
export type EndpointByMethod = {
  get: {
    "/api/v1/drafts": Endpoints.get_List_drafts;
    "/api/v1/drafts/{assessment_id}": Endpoints.get_Get_draft;
    "/api/v1/leads/{lead_id}": Endpoints.get_Lead_detail;
    "/api/v1/llm/providers": Endpoints.get_List_providers;
    "/api/v1/llm/providers/{provider_id}": Endpoints.get_Get_provider;
    "/api/v1/llm/tiers": Endpoints.get_List_tiers;
    "/api/v1/me": Endpoints.get_Me;
    "/api/v1/profiles": Endpoints.get_List_profiles;
    "/api/v1/profiles/{profile_id}": Endpoints.get_Get_profile;
    "/api/v1/profiles/{profile_id}/export": Endpoints.get_Export_profile;
    "/api/v1/profiles/{profile_id}/leads": Endpoints.get_List_leads;
    "/api/v1/profiles/{profile_id}/leads/export": Endpoints.get_Export_leads;
    "/api/v1/profiles/{profile_id}/revisions": Endpoints.get_List_revisions;
    "/api/v1/profiles/{profile_id}/revisions/{number}": Endpoints.get_Get_revision;
    "/api/v1/quality": Endpoints.get_Quality;
    "/api/v1/sources": Endpoints.get_List_sources;
  };
  patch: {
    "/api/v1/drafts/{assessment_id}": Endpoints.patch_Edit_draft;
    "/api/v1/leads/{lead_id}/status": Endpoints.patch_Change_lead_status;
    "/api/v1/llm/providers/{provider_id}": Endpoints.patch_Update_provider;
  };
  post: {
    "/api/v1/drafts/{assessment_id}/approve": Endpoints.post_Approve_draft;
    "/api/v1/drafts/{assessment_id}/reject": Endpoints.post_Reject_draft;
    "/api/v1/leads/{lead_id}/company/refresh": Endpoints.post_Refresh_company;
    "/api/v1/leads/{lead_id}/summary/retry": Endpoints.post_Retry_lead_summary;
    "/api/v1/llm/providers": Endpoints.post_Create_provider;
    "/api/v1/llm/providers/{provider_id}/disable": Endpoints.post_Disable_provider;
    "/api/v1/llm/providers/{provider_id}/enable": Endpoints.post_Enable_provider;
    "/api/v1/llm/providers/{provider_id}/test": Endpoints.post_Test_provider;
    "/api/v1/profiles": Endpoints.post_Create_profile;
    "/api/v1/profiles/import": Endpoints.post_Import_profile;
    "/api/v1/profiles/{profile_id}/activate": Endpoints.post_Activate_profile;
    "/api/v1/profiles/{profile_id}/deactivate": Endpoints.post_Deactivate_profile;
    "/api/v1/sources/{source}/run": Endpoints.post_Run_source;
  };
  delete: {
    "/api/v1/llm/providers/{provider_id}": Endpoints.delete_Delete_provider;
    "/api/v1/llm/tiers/{tier}": Endpoints.delete_Unassign_tier;
  };
  put: {
    "/api/v1/llm/tiers/{tier}": Endpoints.put_Assign_tier;
    "/api/v1/profiles/{profile_id}": Endpoints.put_Update_profile;
  };
};

// </EndpointByMethod>

// <EndpointByMethod.Shorthands>
export type GetEndpoints = EndpointByMethod["get"];
export type PatchEndpoints = EndpointByMethod["patch"];
export type PostEndpoints = EndpointByMethod["post"];
export type DeleteEndpoints = EndpointByMethod["delete"];
export type PutEndpoints = EndpointByMethod["put"];
// </EndpointByMethod.Shorthands>

// <ApiClientTypes>
export type EndpointParameters = {
  body?: unknown;
  query?: Record<string, unknown>;
  header?: Record<string, unknown>;
  path?: Record<string, unknown>;
};

export type MutationMethod = "post" | "put" | "patch" | "delete";
export type Method = "get" | "head" | "options" | MutationMethod;

type RequestFormat = "json" | "form-data" | "form-url" | "binary" | "text";

export type DefaultEndpoint = {
  parameters?: EndpointParameters | undefined;
  responses?: Record<string, unknown>;
  responseHeaders?: Record<string, unknown>;
};

export type Endpoint<TConfig extends DefaultEndpoint = DefaultEndpoint> = {
  operationId: string;
  method: Method;
  path: string;
  requestFormat: RequestFormat;
  parameters?: TConfig["parameters"];
  meta: {
    alias: string;
    hasParameters: boolean;
    areParametersRequired: boolean;
  };
  responses?: TConfig["responses"];
  responseHeaders?: TConfig["responseHeaders"];
};

export interface Fetcher {
  decodePathParams?: (path: string, pathParams: Record<string, string>) => string;
  encodeSearchParams?: (searchParams: Record<string, unknown> | undefined) => URLSearchParams;
  //
  fetch: (input: {
    method: Method;
    url: URL;
    urlSearchParams?: URLSearchParams | undefined;
    parameters?: EndpointParameters | undefined;
    path: string;
    overrides?: RequestInit;
    throwOnStatusError?: boolean;
  }) => Promise<Response>;
  parseResponseData?: (response: Response) => Promise<unknown>;
}

export const successStatusCodes = [
  200, 201, 202, 203, 204, 205, 206, 207, 208, 226, 300, 301, 302, 303, 304, 305, 306, 307, 308,
] as const;
export type SuccessStatusCode = (typeof successStatusCodes)[number];

export const errorStatusCodes = [
  400, 401, 402, 403, 404, 405, 406, 407, 408, 409, 410, 411, 412, 413, 414, 415, 416, 417, 418, 421, 422, 423, 424,
  425, 426, 428, 429, 431, 451, 500, 501, 502, 503, 504, 505, 506, 507, 508, 510, 511,
] as const;
export type ErrorStatusCode = (typeof errorStatusCodes)[number];

// Taken from https://github.com/unjs/fetchdts/blob/ec4eaeab5d287116171fc1efd61f4a1ad34e4609/src/fetch.ts#L3
export interface TypedHeaders<TypedHeaderValues extends Record<string, string> | unknown>
  extends Omit<Headers, "append" | "delete" | "get" | "getSetCookie" | "has" | "set" | "forEach"> {
  /** [MDN Reference](https://developer.mozilla.org/docs/Web/API/Headers/append) */
  append: <Name extends Extract<keyof TypedHeaderValues, string> | (string & {})>(
    name: Name,
    value: Lowercase<Name> extends keyof TypedHeaderValues ? TypedHeaderValues[Lowercase<Name>] : string,
  ) => void;
  /** [MDN Reference](https://developer.mozilla.org/docs/Web/API/Headers/delete) */
  delete: <Name extends Extract<keyof TypedHeaderValues, string> | (string & {})>(name: Name) => void;
  /** [MDN Reference](https://developer.mozilla.org/docs/Web/API/Headers/get) */
  get: <Name extends Extract<keyof TypedHeaderValues, string> | (string & {})>(
    name: Name,
  ) => (Lowercase<Name> extends keyof TypedHeaderValues ? TypedHeaderValues[Lowercase<Name>] : string) | null;
  /** [MDN Reference](https://developer.mozilla.org/docs/Web/API/Headers/getSetCookie) */
  getSetCookie: () => string[];
  /** [MDN Reference](https://developer.mozilla.org/docs/Web/API/Headers/has) */
  has: <Name extends Extract<keyof TypedHeaderValues, string> | (string & {})>(name: Name) => boolean;
  /** [MDN Reference](https://developer.mozilla.org/docs/Web/API/Headers/set) */
  set: <Name extends Extract<keyof TypedHeaderValues, string> | (string & {})>(
    name: Name,
    value: Lowercase<Name> extends keyof TypedHeaderValues ? TypedHeaderValues[Lowercase<Name>] : string,
  ) => void;
  forEach: (
    callbackfn: (
      value: TypedHeaderValues[keyof TypedHeaderValues] | (string & {}),
      key: Extract<keyof TypedHeaderValues, string> | (string & {}),
      parent: TypedHeaders<TypedHeaderValues>,
    ) => void,
    thisArg?: any,
  ) => void;
}

/** @see https://developer.mozilla.org/en-US/docs/Web/API/Response */
export interface TypedSuccessResponse<TSuccess, TStatusCode, THeaders>
  extends Omit<Response, "ok" | "status" | "json" | "headers"> {
  ok: true;
  status: TStatusCode;
  headers: never extends THeaders ? Headers : TypedHeaders<THeaders>;
  data: TSuccess;
  /** [MDN Reference](https://developer.mozilla.org/en-US/docs/Web/API/Response/json) */
  json: () => Promise<TSuccess>;
}

/** @see https://developer.mozilla.org/en-US/docs/Web/API/Response */
export interface TypedErrorResponse<TData, TStatusCode, THeaders>
  extends Omit<Response, "ok" | "status" | "json" | "headers"> {
  ok: false;
  status: TStatusCode;
  headers: never extends THeaders ? Headers : TypedHeaders<THeaders>;
  data: TData;
  /** [MDN Reference](https://developer.mozilla.org/en-US/docs/Web/API/Response/json) */
  json: () => Promise<TData>;
}

export type TypedApiResponse<TAllResponses extends Record<string | number, unknown> = {}, THeaders = {}> = {
  [K in keyof TAllResponses]: K extends string
    ? K extends `${infer TStatusCode extends number}`
      ? TStatusCode extends SuccessStatusCode
        ? TypedSuccessResponse<TAllResponses[K], TStatusCode, K extends keyof THeaders ? THeaders[K] : never>
        : TypedErrorResponse<TAllResponses[K], TStatusCode, K extends keyof THeaders ? THeaders[K] : never>
      : never
    : K extends number
      ? K extends SuccessStatusCode
        ? TypedSuccessResponse<TAllResponses[K], K, K extends keyof THeaders ? THeaders[K] : never>
        : TypedErrorResponse<TAllResponses[K], K, K extends keyof THeaders ? THeaders[K] : never>
      : never;
}[keyof TAllResponses];

export type SafeApiResponse<TEndpoint> = TEndpoint extends { responses: infer TResponses }
  ? TResponses extends Record<string, unknown>
    ? TypedApiResponse<TResponses, TEndpoint extends { responseHeaders: infer THeaders } ? THeaders : never>
    : never
  : never;

export type InferResponseByStatus<TEndpoint, TStatusCode> = Extract<
  SafeApiResponse<TEndpoint>,
  { status: TStatusCode }
>;

type RequiredKeys<T> = {
  [P in keyof T]-?: undefined extends T[P] ? never : P;
}[keyof T];

type MaybeOptionalArg<T> = RequiredKeys<T> extends never ? [config?: T] : [config: T];
type NotNever<T> = [T] extends [never] ? false : true;

// </ApiClientTypes>

// <TypedStatusError>
export class TypedStatusError<TData = unknown> extends Error {
  response: TypedErrorResponse<TData, ErrorStatusCode, unknown>;
  status: number;
  constructor(response: TypedErrorResponse<TData, ErrorStatusCode, unknown>) {
    super(`HTTP ${response.status}: ${response.statusText}`);
    this.name = "TypedStatusError";
    this.response = response;
    this.status = response.status;
  }
}
// </TypedStatusError>

// <ApiClient>
export class ApiClient {
  baseUrl: string = "";
  successStatusCodes = successStatusCodes;
  errorStatusCodes = errorStatusCodes;

  constructor(public fetcher: Fetcher) {}

  setBaseUrl(baseUrl: string) {
    this.baseUrl = baseUrl;
    return this;
  }

  /**
   * Replace path parameters in URL
   * Supports both OpenAPI format {param} and Express format :param
   */
  defaultDecodePathParams = (url: string, params: Record<string, string>): string => {
    return url
      .replace(/{(\w+)}/g, (_, key: string) => params[key] || `{${key}}`)
      .replace(/:([a-zA-Z0-9_]+)/g, (_, key: string) => params[key] || `:${key}`);
  };

  /** Uses URLSearchParams, skips null/undefined values */
  defaultEncodeSearchParams = (queryParams: Record<string, unknown> | undefined): URLSearchParams | undefined => {
    if (!queryParams) return;

    const searchParams = new URLSearchParams();
    Object.entries(queryParams).forEach(([key, value]) => {
      if (value != null) {
        // Skip null/undefined values
        if (Array.isArray(value)) {
          value.forEach((val) => val != null && searchParams.append(key, String(val)));
        } else {
          searchParams.append(key, String(value));
        }
      }
    });

    return searchParams;
  };

  defaultParseResponseData = async (response: Response): Promise<unknown> => {
    const contentType = response.headers.get("content-type") ?? "";
    if (contentType.startsWith("text/")) {
      return await response.text();
    }

    if (contentType === "application/octet-stream") {
      return await response.arrayBuffer();
    }

    if (
      contentType.includes("application/json") ||
      (contentType.includes("application/") && contentType.includes("json")) ||
      contentType === "*/*"
    ) {
      try {
        return await response.json();
      } catch {
        return undefined;
      }
    }

    return;
  };

  // <ApiClient.get>
  get<Path extends keyof GetEndpoints, TEndpoint extends GetEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
    >
  ): Promise<Extract<InferResponseByStatus<TEndpoint, SuccessStatusCode>, { data: {} }>["data"]>;

  get<Path extends keyof GetEndpoints, TEndpoint extends GetEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
    >
  ): Promise<SafeApiResponse<TEndpoint>>;

  get<Path extends keyof GetEndpoints, _TEndpoint extends GetEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<any>
  ): Promise<any> {
    return this.request("get", path, ...params);
  }
  // </ApiClient.get>

  // <ApiClient.patch>
  patch<Path extends keyof PatchEndpoints, TEndpoint extends PatchEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
    >
  ): Promise<Extract<InferResponseByStatus<TEndpoint, SuccessStatusCode>, { data: {} }>["data"]>;

  patch<Path extends keyof PatchEndpoints, TEndpoint extends PatchEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
    >
  ): Promise<SafeApiResponse<TEndpoint>>;

  patch<Path extends keyof PatchEndpoints, _TEndpoint extends PatchEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<any>
  ): Promise<any> {
    return this.request("patch", path, ...params);
  }
  // </ApiClient.patch>

  // <ApiClient.post>
  post<Path extends keyof PostEndpoints, TEndpoint extends PostEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
    >
  ): Promise<Extract<InferResponseByStatus<TEndpoint, SuccessStatusCode>, { data: {} }>["data"]>;

  post<Path extends keyof PostEndpoints, TEndpoint extends PostEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
    >
  ): Promise<SafeApiResponse<TEndpoint>>;

  post<Path extends keyof PostEndpoints, _TEndpoint extends PostEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<any>
  ): Promise<any> {
    return this.request("post", path, ...params);
  }
  // </ApiClient.post>

  // <ApiClient.delete>
  delete<Path extends keyof DeleteEndpoints, TEndpoint extends DeleteEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
    >
  ): Promise<Extract<InferResponseByStatus<TEndpoint, SuccessStatusCode>, { data: {} }>["data"]>;

  delete<Path extends keyof DeleteEndpoints, TEndpoint extends DeleteEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
    >
  ): Promise<SafeApiResponse<TEndpoint>>;

  delete<Path extends keyof DeleteEndpoints, _TEndpoint extends DeleteEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<any>
  ): Promise<any> {
    return this.request("delete", path, ...params);
  }
  // </ApiClient.delete>

  // <ApiClient.put>
  put<Path extends keyof PutEndpoints, TEndpoint extends PutEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
    >
  ): Promise<Extract<InferResponseByStatus<TEndpoint, SuccessStatusCode>, { data: {} }>["data"]>;

  put<Path extends keyof PutEndpoints, TEndpoint extends PutEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
    >
  ): Promise<SafeApiResponse<TEndpoint>>;

  put<Path extends keyof PutEndpoints, _TEndpoint extends PutEndpoints[Path]>(
    path: Path,
    ...params: MaybeOptionalArg<any>
  ): Promise<any> {
    return this.request("put", path, ...params);
  }
  // </ApiClient.put>

  // <ApiClient.request>
  /**
   * Generic request method with full type-safety for any endpoint
   */
  request<
    TMethod extends keyof EndpointByMethod,
    TPath extends keyof EndpointByMethod[TMethod],
    TEndpoint extends EndpointByMethod[TMethod][TPath],
  >(
    method: TMethod,
    path: TPath,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: false; throwOnStatusError?: boolean }
    >
  ): Promise<Extract<InferResponseByStatus<TEndpoint, SuccessStatusCode>, { data: {} }>["data"]>;

  request<
    TMethod extends keyof EndpointByMethod,
    TPath extends keyof EndpointByMethod[TMethod],
    TEndpoint extends EndpointByMethod[TMethod][TPath],
  >(
    method: TMethod,
    path: TPath,
    ...params: MaybeOptionalArg<
      TEndpoint extends { parameters: infer UParams }
        ? NotNever<UParams> extends true
          ? UParams & { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
          : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
        : { overrides?: RequestInit; withResponse?: true; throwOnStatusError?: boolean }
    >
  ): Promise<SafeApiResponse<TEndpoint>>;

  request<
    TMethod extends keyof EndpointByMethod,
    TPath extends keyof EndpointByMethod[TMethod],
    TEndpoint extends EndpointByMethod[TMethod][TPath],
  >(method: TMethod, path: TPath, ...params: MaybeOptionalArg<any>): Promise<any> {
    const requestParams = params[0];
    const withResponse = requestParams?.withResponse;
    const {
      withResponse: _,
      throwOnStatusError = withResponse ? false : true,
      overrides,
      ...fetchParams
    } = requestParams || {};

    const parametersToSend: EndpointParameters = {};
    if (requestParams?.body !== undefined) (parametersToSend as any).body = requestParams.body;
    if (requestParams?.query !== undefined) (parametersToSend as any).query = requestParams.query;
    if (requestParams?.header !== undefined) (parametersToSend as any).header = requestParams.header;
    if (requestParams?.path !== undefined) (parametersToSend as any).path = requestParams.path;

    const resolvedPath = (this.fetcher.decodePathParams ?? this.defaultDecodePathParams)(
      this.baseUrl + (path as string),
      (parametersToSend.path ?? {}) as Record<string, string>,
    );
    const url = new URL(resolvedPath);
    const urlSearchParams = (this.fetcher.encodeSearchParams ?? this.defaultEncodeSearchParams)(parametersToSend.query);

    const promise = this.fetcher
      .fetch({
        method: method,
        path: path as string,
        url,
        urlSearchParams,
        parameters: Object.keys(fetchParams).length ? fetchParams : undefined,
        overrides,
        throwOnStatusError,
      })
      .then(async (response) => {
        const data = await (this.fetcher.parseResponseData ?? this.defaultParseResponseData)(response);
        const typedResponse = Object.assign(response, {
          data: data,
          json: () => Promise.resolve(data),
        }) as SafeApiResponse<TEndpoint>;

        if (throwOnStatusError && errorStatusCodes.includes(response.status as never)) {
          throw new TypedStatusError(typedResponse as never);
        }

        return withResponse ? typedResponse : data;
      });

    return promise as Extract<InferResponseByStatus<TEndpoint, SuccessStatusCode>, { data: {} }>["data"];
  }
  // </ApiClient.request>
}

export function createApiClient(fetcher: Fetcher, baseUrl?: string) {
  return new ApiClient(fetcher).setBaseUrl(baseUrl ?? "");
}

/**
 Example usage:
 const api = createApiClient((method, url, params) =>
   fetch(url, { method, body: JSON.stringify(params) }).then((res) => res.json()),
 );
 api.get("/users").then((users) => console.log(users));
 api.post("/users", { body: { name: "John" } }).then((user) => console.log(user));
 api.put("/users/:id", { path: { id: 1 }, body: { name: "John" } }).then((user) => console.log(user));

 // With error handling
 const result = await api.get("/users/{id}", { path: { id: "123" }, withResponse: true });
 if (result.ok) {
   // Access data directly
   const user = result.data;
   console.log(user);

   // Or use the json() method for compatibility
   const userFromJson = await result.json();
   console.log(userFromJson);
 } else {
   const error = result.data;
   console.error(`Error ${result.status}:`, error);
 }
*/

// </ApiClient>
