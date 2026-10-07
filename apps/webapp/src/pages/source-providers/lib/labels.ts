import type { ProviderSummary } from '#/pages/source-providers/lib/types'

export const CATEGORY_ORDER = [
	'public_procurement',
	'jobs_and_missions',
	'technical_signals',
	'companies',
] as const

const CATEGORY_LABELS: Readonly<Record<string, string>> = {
	public_procurement: 'Marchés publics',
	jobs_and_missions: 'Offres et missions',
	technical_signals: 'Signaux techniques',
	companies: 'Entreprises',
}

const STATUS_LABELS: Readonly<Record<string, string>> = {
	not_configured: 'Non configuré',
	untested: 'À tester',
	ready: 'Prêt',
	failing: 'En échec',
}

const AUTH_LABELS: Readonly<Record<string, string>> = {
	none: 'Sans authentification',
	api_key: "Clé d'API",
	oauth2_client_credentials: 'OAuth2 · compte de service',
	oauth2_authorization_code: 'OAuth2 · compte connecté',
}

const CALLBACK_ERRORS: Readonly<Record<string, string>> = {
	invalid_state:
		"La connexion a échoué : la demande d'autorisation est inconnue ou déjà utilisée. Recommencez la connexion.",
	expired_state:
		'La demande de connexion a expiré (10 minutes). Recommencez la connexion.',
	exchange_failed:
		"Le fournisseur a refusé l'échange du code d'autorisation. Vérifiez l'identifiant client, le secret et l'URL de rappel déclarée.",
	denied: "L'accès au compte a été refusé chez le fournisseur.",
	invalid_request: 'La réponse du fournisseur est incomplète.',
	internal: 'Une erreur interne a interrompu la connexion du compte.',
}

export function categoryLabel(category: string): string {
	return CATEGORY_LABELS[category] ?? category
}

export function statusLabel(status: string): string {
	return STATUS_LABELS[status] ?? status
}

export function authLabel(kind: string): string {
	return AUTH_LABELS[kind] ?? kind
}

export function callbackErrorMessage(code: string): string {
	return CALLBACK_ERRORS[code] ?? 'La connexion du compte a échoué.'
}

export interface CategoryGroup {
	category: string
	title: string
	providers: ProviderSummary[]
}

export function groupByCategory(
	providers: readonly ProviderSummary[],
): CategoryGroup[] {
	const known: string[] = [...CATEGORY_ORDER]
	const extra = [
		...new Set(
			providers
				.map((provider) => provider.category)
				.filter((category) => !known.includes(category)),
		),
	]
	return [...known, ...extra]
		.map((category) => ({
			category,
			title: categoryLabel(category),
			providers: providers.filter((provider) => provider.category === category),
		}))
		.filter((group) => group.providers.length > 0)
}

export function capabilityLabels(provider: ProviderSummary): string[] {
	const labels: string[] = []
	if (provider.capabilities.keyword_search) labels.push('Mots-clés')
	if (provider.capabilities.filters.remote) labels.push('Télétravail')
	if (provider.capabilities.filters.countries) labels.push('Pays')
	if (provider.capabilities.filters.contract) labels.push('Contrat')
	return labels
}
