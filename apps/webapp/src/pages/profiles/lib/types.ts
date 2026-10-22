import type { Schemas } from '#/api/api.client'

export type ProfileSummary = Schemas.ProfileSummaryDto
export type ProfileDetail = Schemas.ProfileDetailDto
export type RevisionSummary = Schemas.RevisionSummaryDto
export type Revision = Schemas.RevisionDto
export type Trial = Schemas.TrialDto
export type TrialDetail = Schemas.TrialDetailDto
export type TrialLead = Schemas.TrialLeadDto
export type VersionSummary = Schemas.VersionSummaryDto

export interface Problem {
	path: string
	message: string
}

export type DefinitionParse =
	| { ok: true; value: Record<string, unknown> }
	| { ok: false; problems: Problem[] }
