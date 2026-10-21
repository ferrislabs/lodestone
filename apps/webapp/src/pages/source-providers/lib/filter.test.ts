import { describe, expect, it } from 'vitest'

import {
	categoryCounts,
	filterProviders,
	matchesQuery,
	normalizeText,
} from '#/pages/source-providers/lib/filter'
import { summary } from '#/pages/source-providers/lib/fixtures'

const PROVIDERS = [
	summary({
		key: 'ats',
		name: 'Pages carrières',
		category: 'jobs_and_missions',
	}),
	summary({
		key: 'boamp',
		name: 'BOAMP',
		description: 'Avis de marchés publics.',
		category: 'public_procurement',
	}),
	summary({
		key: 'ted',
		name: 'TED',
		description: 'Appels d’offres européens.',
		category: 'public_procurement',
		capabilities: {
			keyword_search: false,
			filters: { remote: true, countries: false, contract: false },
		},
	}),
	summary({
		key: 'dns_cname',
		name: 'DNS CNAME',
		category: 'technical_signals',
	}),
]

describe('normalizeText', () => {
	it('drops accents and case', () => {
		expect(normalizeText('  Marchés PUBLICS ')).toBe('marches publics')
	})
})

describe('matchesQuery', () => {
	it('matches everything on an empty query', () => {
		expect(matchesQuery(PROVIDERS[0], '  ')).toBe(true)
	})

	it('ignores accents and case in name and description', () => {
		expect(matchesQuery(PROVIDERS[0], 'CARRIERES')).toBe(true)
		expect(matchesQuery(PROVIDERS[1], 'marche')).toBe(true)
	})

	it('matches the key and the capability labels', () => {
		expect(matchesQuery(PROVIDERS[3], 'dns_cname')).toBe(true)
		expect(matchesQuery(PROVIDERS[2], 'teletravail')).toBe(true)
		expect(matchesQuery(PROVIDERS[1], 'teletravail')).toBe(false)
	})

	it('requires every word to match', () => {
		expect(matchesQuery(PROVIDERS[1], 'boamp avis')).toBe(true)
		expect(matchesQuery(PROVIDERS[1], 'boamp zzz')).toBe(false)
	})
})

describe('filterProviders', () => {
	it('keeps catalog order when unfiltered', () => {
		const keys = filterProviders(PROVIDERS, {
			category: null,
			status: null,
			query: '',
		}).map((provider) => provider.key)
		expect(keys).toEqual(['boamp', 'ted', 'ats', 'dns_cname'])
	})

	it('combines the category and the query', () => {
		const keys = filterProviders(PROVIDERS, {
			category: 'public_procurement',
			status: null,
			query: 'ted',
		}).map((provider) => provider.key)
		expect(keys).toEqual(['ted'])
	})

	it('filters by status combined with the query', () => {
		const withStatus = [
			summary({ key: 'a', name: 'Alpha', status: 'ready' }),
			summary({ key: 'b', name: 'Beta', status: 'failing' }),
			summary({ key: 'c', name: 'Alpine', status: 'ready' }),
		]
		const keys = filterProviders(withStatus, {
			category: null,
			status: 'ready',
			query: 'alp',
		}).map((provider) => provider.key)
		expect(keys).toEqual(['a', 'c'])
		expect(
			filterProviders(withStatus, {
				category: null,
				status: 'failing',
				query: 'alp',
			}),
		).toEqual([])
	})

	it('returns nothing when no provider matches', () => {
		expect(
			filterProviders(PROVIDERS, {
				category: 'companies',
				status: null,
				query: '',
			}),
		).toEqual([])
	})
})

describe('categoryCounts', () => {
	it('counts the providers matching the query per category', () => {
		expect(
			categoryCounts(PROVIDERS, '').map((c) => [c.category, c.count]),
		).toEqual([
			['public_procurement', 2],
			['jobs_and_missions', 1],
			['technical_signals', 1],
		])
		expect(categoryCounts(PROVIDERS, 'ted')[0]).toMatchObject({
			title: 'Marchés publics',
			count: 1,
		})
	})
})
