import { describe, expect, it } from 'vitest'

import { criteriaMatrix } from '#/pages/profiles/lib/matrix'
import { summary } from '#/pages/source-providers/lib/fixtures'

const CATALOG = [
	summary({ key: 'boamp', name: 'BOAMP' }),
	summary({
		key: 'ted',
		name: 'TED',
		capabilities: {
			keyword_search: true,
			filters: { remote: false, countries: true, contract: false },
		},
	}),
	summary({
		key: 'france_travail',
		name: 'France Travail',
		capabilities: {
			keyword_search: true,
			filters: { remote: false, countries: false, contract: true },
		},
	}),
	summary({
		key: 'dns_cname',
		name: 'DNS CNAME',
		capabilities: {
			keyword_search: false,
			filters: { remote: false, countries: false, contract: false },
		},
	}),
]

describe('criteriaMatrix', () => {
	it('applies keywords natively and everything else after collection by default', () => {
		const [boamp] = criteriaMatrix(CATALOG, ['boamp'])
		expect(boamp?.cells).toEqual({
			keywords: 'native',
			exclusions: 'after',
			remote: 'after',
			countries: 'after',
			contract: 'after',
		})
	})

	it('marks filters the provider supports as native', () => {
		const columns = criteriaMatrix(CATALOG, ['ted', 'france_travail'])
		expect(columns.map((column) => column.cells.countries)).toEqual([
			'native',
			'after',
		])
		expect(columns.map((column) => column.cells.contract)).toEqual([
			'after',
			'native',
		])
	})

	it('flags a provider without keyword search as not applicable', () => {
		const [dns] = criteriaMatrix(CATALOG, ['dns_cname'])
		expect(Object.values(dns?.cells ?? {})).toEqual(Array(5).fill('none'))
	})

	it('keeps the order of the selected keys and skips unknown ones', () => {
		expect(
			criteriaMatrix(CATALOG, ['ted', 'ghost', 'boamp']).map(
				(column) => column.key,
			),
		).toEqual(['ted', 'boamp'])
	})
})
