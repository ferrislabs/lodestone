import type { JsonObject } from '#/lib/schema-form'

export const IAM: JsonObject = {
	agents: [
		{
			id: 'probe',
			enabled: true,
			source: {
				kind: 'http_probe',
				fingerprints: [
					{
						product: 'keycloak',
						debt_markers: [{ id: 'legacy_auth_path' }, { id: 'basic_auth' }],
					},
					{ product: 'auth0' },
				],
			},
		},
		{
			id: 'dns',
			enabled: false,
			source: {
				kind: 'dns_cname',
				patterns: [{ provider: 'okta' }, { provider: 'okta' }],
			},
		},
	],
	scoring: { factors: [] },
}
