import { describe, expect, it } from 'vitest'

import { newAgent } from '#/pages/profiles/lib/agents'
import { COUNTRY_CODES } from '#/pages/profiles/lib/countries'
import {
	agentContracts,
	agentCountries,
	agentKeywordCount,
	agentRemote,
	allContracts,
	capabilityHint,
	capabilityText,
	isReadyProvider,
	notReadyProviders,
	providerName,
	providerStatusOf,
	searchProviders,
	withContract,
	withCountries,
	withFilter,
	withProvider,
} from '#/pages/profiles/lib/search-agent'
import { summary } from '#/pages/source-providers/lib/fixtures'

const CATALOG = [
	summary({ key: 'boamp', name: 'BOAMP', status: 'ready' }),
	summary({
		key: 'france_travail',
		name: 'France Travail',
		status: 'failing',
		capabilities: {
			keyword_search: true,
			filters: { remote: false, countries: false, contract: true },
		},
	}),
	summary({
		key: 'ted',
		name: 'TED',
		status: 'ready',
		capabilities: {
			keyword_search: true,
			filters: { remote: false, countries: true, contract: false },
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

describe('search agent catalog helpers', () => {
	it('lists only the providers that search by keywords', () => {
		expect(searchProviders(CATALOG).map((provider) => provider.key)).toEqual([
			'boamp',
			'france_travail',
			'ted',
		])
	})

	it('reads names and statuses, unknown providers included', () => {
		expect(providerName(CATALOG, 'ted')).toBe('TED')
		expect(providerName(CATALOG, 'linkedin')).toBe('linkedin')
		expect(providerStatusOf(CATALOG, 'france_travail')).toBe('failing')
		expect(providerStatusOf(CATALOG, 'linkedin')).toBe('unknown')
		expect(isReadyProvider(CATALOG, 'boamp')).toBe(true)
		expect(isReadyProvider(CATALOG, 'france_travail')).toBe(false)
		expect(
			notReadyProviders(['boamp', 'france_travail', 'linkedin'], CATALOG),
		).toEqual(['france_travail', 'linkedin'])
	})

	it('splits the filters into native and applied after collection', () => {
		const [boamp, travail, ted] = CATALOG
		expect(capabilityText(capabilityHint(boamp as never))).toBe(
			'Filtré nativement : aucun ; appliqué après la collecte : Télétravail, Pays, Contrat',
		)
		expect(capabilityText(capabilityHint(travail as never))).toBe(
			'Filtré nativement : Contrat ; appliqué après la collecte : Télétravail, Pays',
		)
		expect(capabilityText(capabilityHint(ted as never))).toBe(
			'Filtré nativement : Pays ; appliqué après la collecte : Télétravail, Contrat',
		)
	})
})

describe('search agent edits', () => {
	it('attaches and detaches providers without duplicates', () => {
		const agent = newAgent('search', [], ['boamp'])
		const both = withProvider(agent, 'ted', true)
		expect(both.providers).toEqual(['boamp', 'ted'])
		expect(withProvider(both, 'ted', true).providers).toEqual(['boamp', 'ted'])
		expect(withProvider(both, 'boamp', false).providers).toEqual(['ted'])
	})

	it('sets the remote filter and the contracts', () => {
		const agent = newAgent('search', [])
		expect(agentRemote(agent)).toBe('any')
		const remote = withFilter(agent, 'remote', 'full_remote')
		expect(agentRemote(remote)).toBe('full_remote')
		const contracts = withContract(
			withContract(remote, 'freelance', true),
			'employee',
			true,
		)
		expect(agentContracts(contracts)).toEqual(['freelance', 'employee'])
		expect(agentContracts(withContract(contracts, 'freelance', false))).toEqual(
			['employee'],
		)
		expect(agentRemote(contracts)).toBe('full_remote')
	})

	it('treats Tous as the absence of contract filter and clears the others', () => {
		const agent = newAgent('search', [])
		expect(allContracts(agent)).toBe(true)
		const picked = withContract(agent, 'freelance', true)
		expect(allContracts(picked)).toBe(false)
		const cleared = withContract(picked, 'any', true)
		expect(agentContracts(cleared)).toEqual([])
		expect(allContracts(cleared)).toBe(true)
	})

	it('drops any when a specific contract is chosen', () => {
		const agent = withFilter(newAgent('search', []), 'contracts', ['any'])
		expect(agentContracts(withContract(agent, 'employee', true))).toEqual([
			'employee',
		])
	})

	it('reads and writes the country codes', () => {
		const codes = COUNTRY_CODES.slice(0, 2)
		const agent = withCountries(newAgent('search', []), codes)
		expect(agentCountries(agent)).toEqual(codes)
		expect(agentRemote(agent)).toBe('any')
		expect(agentCountries(undefined)).toEqual([])
	})

	it('counts the non empty keywords', () => {
		expect(agentKeywordCount({ keywords: ['sso', ' ', 'mfa'] })).toBe(2)
		expect(agentKeywordCount(undefined)).toBe(0)
	})
})
