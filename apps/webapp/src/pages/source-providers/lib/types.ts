import type { Schemas } from '#/api/api.client'

export type ProviderSummary = Schemas.ProviderSummaryDto
export type ProviderDetail = Schemas.ProviderDetailDto
export type ProviderField = Schemas.FieldDto
export type SecretState = Schemas.SecretStateDto

export type ProviderStatus = 'not_configured' | 'untested' | 'ready' | 'failing'

export type FieldValues = Record<string, string | boolean>
export type SecretInputs = Record<string, string>

export interface SaveBody {
	settings: Record<string, string | number | boolean>
	secrets: Record<string, string>
}
