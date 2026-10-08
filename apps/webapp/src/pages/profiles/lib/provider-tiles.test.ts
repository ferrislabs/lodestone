import { describe, expect, it } from 'vitest'

import { providerTiles } from '#/pages/profiles/lib/provider-tiles'
import { summary } from '#/pages/source-providers/lib/fixtures'

const CATALOG = [
	summary({ key: 'boamp', name: 'BOAMP', status: 'ready' }),
	summary({ key: 'france_travail', name: 'France Travail', status: 'failing' }),
	summary({ key: 'ted', name: 'TED', status: 'untested' }),
	summary({
		key: 'ats',
		name: "Offres d'emploi de l'entreprise",
		category: 'companies',
		status: 'ready',
		capabilities: {
			keyword_search: false,
			filters: { remote: false, countries: false, contract: false },
		},
	}),
	summary({
		key: 'http_probe',
		name: 'Sonde HTTP',
		status: 'ready',
		capabilities: {
			keyword_search: false,
			filters: { remote: false, countries: false, contract: false },
		},
	}),
]

describe('provider tiles', () => {
	it('lists the keyword search providers with their state', () => {
		const tiles = providerTiles(CATALOG, ['boamp'])
		expect(
			tiles.map((tile) => [tile.key, tile.state, tile.selectable]),
		).toEqual([
			['boamp', 'selected', true],
			['france_travail', 'unready', false],
			['ted', 'unready', false],
		])
	})

	it('keeps a selected provider selected even when it is not ready', () => {
		const tiles = providerTiles(CATALOG, ['ted'])
		expect(tiles.find((tile) => tile.key === 'ted')).toMatchObject({
			name: 'TED',
			state: 'selected',
			ready: false,
			selectable: true,
		})
	})

	it('offers the company job offers only as a leftover selection to remove', () => {
		expect(providerTiles(CATALOG, []).map((tile) => tile.key)).not.toContain(
			'ats',
		)
		expect(
			providerTiles(CATALOG, ['ats']).find((tile) => tile.key === 'ats'),
		).toMatchObject({
			name: "Offres d'emploi de l'entreprise",
			state: 'selected',
		})
	})

	it('shows a selected provider missing from the catalog', () => {
		const tiles = providerTiles([], ['ats'])
		expect(tiles).toEqual([
			{
				key: 'ats',
				name: 'ats',
				status: 'unknown',
				state: 'selected',
				ready: false,
				selectable: true,
			},
		])
	})
})
