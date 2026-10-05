import type { Schemas } from '#/api/api.client'

export type LeadStatus = Schemas.LeadStatusDto
export type Lead = Schemas.LeadDto
export type LeadDetailData = Schemas.DataEnvelope_LeadDetailDto['data']
export type Signal = Schemas.SignalDto
export type Contact = Schemas.ContactDto
export type Assessment = Schemas.AssessmentDto
export type ScoreContribution = Schemas.ScoreContributionDto
export type ProfileSummary =
	Schemas.DataEnvelope_Vec_ProfileSummaryDto['data'][number]
export type PaginationMeta = Schemas.PaginationMetadata
