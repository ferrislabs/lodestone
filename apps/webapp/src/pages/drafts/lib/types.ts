import type { Schemas } from '#/api/api.client'

export type DraftEntry = Schemas.DraftEntryDto
export type DraftContact = Schemas.DraftContactDto
export type DraftEvidence = Schemas.EvidenceDto
export type { PaginationMeta, ProfileSummary } from '#/pages/leads/lib/types'

export interface DraftValues {
	subject: string
	body: string
}

export interface RejectValues {
	reason: string
}
