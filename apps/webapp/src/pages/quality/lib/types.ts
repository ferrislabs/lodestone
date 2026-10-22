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
export type Calibration = Schemas.CalibrationDto
export type ProfileCalibration = Schemas.ProfileCalibrationDto
export type AgentCalibration = Schemas.AgentCalibrationDto
export type ReliabilityBin = Schemas.ReliabilityBinDto
export type TriageRecall = Schemas.TriageRecallDto
export type DroppedCandidate = Schemas.DroppedCandidateDto
export type DropReason = Schemas.DropReasonDto
export type DropReview = Schemas.DropReviewDto
export type AgentYield = Schemas.AgentYieldDto
export type KeywordYield = Schemas.KeywordYieldDto
export type KeywordSuggestion = Schemas.KeywordSuggestionDto
