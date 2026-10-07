import { describe, expect, it } from 'vitest'

import { summary } from '#/pages/source-providers/lib/fixtures'
import {
	authLabel,
	callbackErrorMessage,
	capabilityLabels,
	categoryLabel,
	groupByCategory,
	statusLabel,
} from '#/pages/source-providers/lib/labels'

describe('labels', () => {
	it('labels each status in French', () => {
		expect(statusLabel('not_configured')).toBe('Non configuré')
		expect(statusLabel('untested')).toBe('À tester')
		expect(statusLabel('ready')).toBe('Prêt')
		expect(statusLabel('failing')).toBe('En échec')
	})

	it('labels the authentication kinds and categories', () => {
		expect(authLabel('none')).toBe('Sans authentification')
		expect(authLabel('api_key')).toBe("Clé d'API")
		expect(authLabel('oauth2_client_credentials')).toBe(
			'OAuth2 · compte de service',
		)
		expect(authLabel('oauth2_authorization_code')).toBe(
			'OAuth2 · compte connecté',
		)
		expect(categoryLabel('jobs_and_missions')).toBe('Offres et missions')
		expect(categoryLabel('other')).toBe('other')
	})

	it('groups providers by category in catalog order and drops empty groups', () => {
		const groups = groupByCategory([
			summary({ key: 'a', category: 'companies' }),
			summary({ key: 'b', category: 'public_procurement' }),
			summary({ key: 'c', category: 'public_procurement' }),
		])
		expect(groups.map((group) => group.title)).toEqual([
			'Marchés publics',
			'Entreprises',
		])
		expect(groups[0]?.providers.map((provider) => provider.key)).toEqual([
			'b',
			'c',
		])
	})

	it('lists the capabilities a provider supports', () => {
		expect(capabilityLabels(summary())).toEqual(['Mots-clés'])
		expect(
			capabilityLabels(
				summary({
					capabilities: {
						keyword_search: false,
						filters: { remote: true, countries: true, contract: true },
					},
				}),
			),
		).toEqual(['Télétravail', 'Pays', 'Contrat'])
	})

	it('explains callback errors', () => {
		expect(callbackErrorMessage('expired_state')).toContain('expiré')
		expect(callbackErrorMessage('unknown')).toBe(
			'La connexion du compte a échoué.',
		)
	})
})
