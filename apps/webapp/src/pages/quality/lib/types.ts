import type { Schemas } from '#/api/api.client'

export type Quality = Schemas.QualityDto
export type VerdictPrecision = Schemas.VerdictPrecisionDto
export type ConfidenceBucket = Schemas.ConfidenceBucketDto
export type AgentQuality = Schemas.AgentQualityDto
export type ProfileQuality = Schemas.ProfileQualityDto
export type VersionPrecision = Schemas.VersionPrecisionDto
export type ReasonCount = Schemas.ReasonCountDto
export type ExclusionCount = Schemas.ExclusionCountDto
export type ExclusionCandidate = Schemas.ExclusionCandidateDto
export type { ProfileSummary } from '#/pages/leads/lib/types'
