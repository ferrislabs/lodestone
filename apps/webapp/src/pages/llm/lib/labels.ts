import type { TierName } from '#/pages/llm/lib/types'

const REASONING_LABELS: Readonly<Record<string, string>> = {
	none: 'Désactivé',
	low: 'Faible',
	medium: 'Moyen',
	high: 'Élevé',
}

const TEST_ERROR_HINTS: Readonly<Record<string, string>> = {
	reasoning_exhausted:
		'Réglez « Raisonnement » sur « Désactivé » dans la configuration du fournisseur.',
}

const TIER_LABELS: Readonly<Record<TierName, string>> = {
	triage: 'Classification (agents de qualification par défaut)',
	qualification: 'Synthèse (agrégateur par défaut)',
}

const TIER_SHORT_LABELS: Readonly<Record<TierName, string>> = {
	triage: 'Classification',
	qualification: 'Synthèse',
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
	reasoning_exhausted: 'Le modèle a consommé tous ses tokens à raisonner',
	reasoning_unsupported: 'Raisonnement non pris en charge',
	other: 'Erreur',
}

export function tierLabel(tier: string): string {
	return TIER_LABELS[tier as TierName] ?? tier
}

export function tierShortLabel(tier: string): string {
	return TIER_SHORT_LABELS[tier as TierName] ?? tier
}

export function providerKindLabel(kind: string): string {
	return PROVIDER_KIND_LABELS[kind] ?? kind
}

export function testErrorLabel(kind: string): string {
	return TEST_ERROR_LABELS[kind] ?? kind
}

export function testErrorHint(kind: string): string | null {
	return TEST_ERROR_HINTS[kind] ?? null
}

export function inFlightLabel(inFlight: number, limit: number): string {
	return `En cours : ${inFlight} / ${limit}`
}

export function reasoningLabel(effort: string): string {
	return REASONING_LABELS[effort] ?? effort
}
