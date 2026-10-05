import type { Schemas } from '#/api/api.client'

export type Provider = Schemas.ProviderDto
export type TierAssignment = Schemas.TierDto
export type TestResult = Schemas.TestResultDto

export const TIERS = ['triage', 'qualification'] as const
export type TierName = (typeof TIERS)[number]

export const PROVIDER_KINDS = [
	'anthropic',
	'mistral',
	'openai_compatible',
] as const

export type KeyMode = 'keep' | 'replace' | 'remove'

export interface ProviderFormValues {
	name: string
	kind: string
	base_url: string
	model: string
	max_retries: string
	timeout_seconds: string
	input_cents_per_mtok: string
	output_cents_per_mtok: string
	api_key: string
}

export type ProviderFieldName = keyof ProviderFormValues

export type TierSelection = Record<TierName, string>

export type TierAction =
	| { tier: TierName; op: 'assign'; providerId: string }
	| { tier: TierName; op: 'unassign' }
