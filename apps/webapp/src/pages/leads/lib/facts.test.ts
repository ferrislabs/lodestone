import { describe, expect, it } from 'vitest'

import { countryName, environmentLabel, hostsSummary } from './facts'
import type { HostFact } from './types'

const host = (environment: string, name = 'a.acme.test'): HostFact => ({
	host: name,
	environment,
	signals: 1,
})

describe('hostsSummary', () => {
	it('says when no host is known', () => {
		expect(hostsSummary([])).toBe('Aucun hôte identifié')
	})

	it('names the single environment of one host', () => {
		expect(hostsSummary([host('development')])).toBe('1 hôte · dev')
	})

	it('names the shared environment of several hosts once', () => {
		expect(
			hostsSummary([
				host('staging', 'a.acme.test'),
				host('staging', 'b.acme.test'),
			]),
		).toBe('2 hôtes · staging')
	})

	it('counts every environment, production first', () => {
		expect(
			hostsSummary([
				host('development', 'a.acme.test'),
				host('production', 'b.acme.test'),
				host('production', 'c.acme.test'),
				host('preproduction', 'd.acme.test'),
			]),
		).toBe('4 hôtes · 2 prod, 1 préprod, 1 dev')
	})
})

describe('environmentLabel', () => {
	it('labels each environment in French and keeps unknown ones', () => {
		expect(environmentLabel('production')).toBe('prod')
		expect(environmentLabel('demo')).toBe('démo')
		expect(environmentLabel('lab')).toBe('lab')
	})
})

describe('countryName', () => {
	it('localises the country in French', () => {
		expect(countryName('FR', 'France')).toBe('France')
		expect(countryName('DE', 'Germany')).toBe('Allemagne')
	})

	it('falls back on an invalid code', () => {
		expect(countryName('not a code', 'Nowhere')).toBe('Nowhere')
	})
})
