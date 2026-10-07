import type {
	ProviderDetail,
	ProviderSummary,
} from '#/pages/source-providers/lib/types'

export function summary(
	overrides: Partial<ProviderSummary> = {},
): ProviderSummary {
	return {
		key: 'boamp',
		name: 'BOAMP',
		description: 'Avis de marchés publics.',
		category: 'public_procurement',
		auth_kind: 'none',
		capabilities: {
			keyword_search: true,
			filters: { remote: false, countries: false, contract: false },
		},
		status: 'not_configured',
		last_test_at: null,
		connected_account: null,
		...overrides,
	}
}

export function detail(
	overrides: Partial<ProviderDetail> = {},
	summaryOverrides: Partial<ProviderSummary> = {},
): ProviderDetail {
	return {
		summary: summary({
			key: 'france_travail',
			name: 'France Travail',
			category: 'jobs_and_missions',
			auth_kind: 'oauth2_client_credentials',
			status: 'ready',
			last_test_at: '2026-10-07T10:00:00Z',
			...summaryOverrides,
		}),
		fields: [
			{
				key: 'client_id',
				label: 'Identifiant client',
				kind: 'text',
				required: true,
				default: null,
				help: "Identifiant de l'application partenaire.",
				options: [],
			},
			{
				key: 'client_secret',
				label: 'Clé secrète',
				kind: 'secret',
				required: true,
				default: null,
				help: '',
				options: [],
			},
			{
				key: 'api_base_url',
				label: "URL de l'API",
				kind: 'url',
				required: false,
				default: 'https://api.example.com/v2',
				help: '',
				options: [],
			},
		],
		settings: {
			client_id: 'client-1',
			api_base_url: 'https://api.example.com/v2',
		},
		secrets: [{ key: 'client_secret', is_set: true, hint: '••••abcd' }],
		scopes: ['api_offresdemploiv2'],
		last_test_message: 'Jeton obtenu et API accessible.',
		terms_note: "Respecter les conditions d'utilisation de l'API.",
		oauth_redirect_uri: null,
		...overrides,
	}
}
