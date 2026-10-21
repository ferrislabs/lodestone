import type { TierName } from '#/pages/llm/lib/types'

const TIER_LABELS: Readonly<Record<TierName, string>> = {
	triage: 'Tri',
	qualification: 'Qualification',
}

const PROVIDER_KIND_LABELS: Readonly<Record<string, string>> = {
	anthropic: 'Anthropic',
	mistral: 'Mistral',
	openai_compatible: 'Compatible OpenAI',
}

const TEST_ERROR_LABELS: Readonly<Record<string, string>> = {
	timeout: 'Délai dépassé',
	rate_limited: 'Débit limité',
	authentication: 'Authentification refusée',
	unreachable: 'Injoignable',
	invalid_response: 'Réponse invalide',
	other: 'Erreur',
}

export function tierLabel(tier: string): string {
	return TIER_LABELS[tier as TierName] ?? tier
}

export function providerKindLabel(kind: string): string {
	return PROVIDER_KIND_LABELS[kind] ?? kind
}

export function testErrorLabel(kind: string): string {
	return TEST_ERROR_LABELS[kind] ?? kind
}
