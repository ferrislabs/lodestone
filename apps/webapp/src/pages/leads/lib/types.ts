import type { Schemas } from '#/api/api.client'

export type LeadStatus = Schemas.LeadStatusDto
export type Lead = Schemas.LeadDto
export type LeadDetailData = Schemas.DataEnvelope_LeadDetailDto['data']
export type LeadFacts = Schemas.LeadFactsDto
export type HostFact = Schemas.HostFactDto
export type Signal = Schemas.SignalDto
export type Contact = Schemas.ContactDto
export type Assessment = Schemas.AssessmentDto
export type ProfileSummary =
	Schemas.DataEnvelope_Vec_ProfileSummaryDto['data'][number]
export type PaginationMeta = Schemas.PaginationMetadata
export type Verdict = Schemas.VerdictDto
export type Exclusion = Schemas.ExclusionDto
export type SummaryState = Schemas.SummaryStateDto
export type AgentFailure = Schemas.AgentFailureDto
export type AgentStop = Schemas.AgentStopDto
export type QualificationResult = Schemas.QualificationResultDto
export type QualificationAnswer = Schemas.QualificationAnswerDto
export type ScoringResult = Schemas.ScoringResultDto
export type ScoringVerdict = NonNullable<ScoringResult['verdict']>
export type VerdictProbabilities = ScoringVerdict['probabilities']
export type Company = NonNullable<LeadDetailData['company']>
export type CompanyOfficial = NonNullable<Company['official']>
export type SiblingLead = Company['siblings'][number]
export type GroupedLead = Lead['grouped_leads'][number]
export type Enrichment = Schemas.EnrichmentDto
export type Ecosystem = Schemas.EcosystemDto
export type EnrichmentStatus = Schemas.EnrichmentStatus
export type EnrichmentWaitingReason = Schemas.EnrichmentWaitingReason
export type EnrichmentWaiting = Schemas.EnrichmentWaitingDto
