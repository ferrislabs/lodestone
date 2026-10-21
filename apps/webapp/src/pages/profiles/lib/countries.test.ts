import { describe, expect, it } from 'vitest'

import {
	COUNTRY_CODES,
	countryName,
	searchCountries,
	withCountry,
	withoutCountry,
} from '#/pages/profiles/lib/countries'

describe('countries', () => {
	it('names an ISO code in french', () => {
		expect(countryName('FR')).toBe('France')
		expect(countryName('DE')).toBe('Allemagne')
		expect(countryName('GB')).toBe('Royaume-Uni')
	})

	it('falls back to the code when it is unknown', () => {
		expect(countryName('QQ')).toBe('QQ')
	})

	it('lists only uppercase two letter codes without duplicates', () => {
		expect(COUNTRY_CODES.length).toBeGreaterThan(200)
		expect(COUNTRY_CODES.every((code) => /^[A-Z]{2}$/.test(code))).toBe(true)
		expect(new Set(COUNTRY_CODES).size).toBe(COUNTRY_CODES.length)
	})

	it('finds countries by french name, ignoring case and accents', () => {
		expect(searchCountries('etats', []).map((country) => country.code)).toEqual(
			['US'],
		)
		expect(searchCountries('ALLEM', [])[0]?.code).toBe('DE')
	})

	it('finds a country by its code', () => {
		expect(searchCountries('be', [])[0]?.code).toBe('BE')
	})

	it('puts names starting with the query first and skips chosen ones', () => {
		const found = searchCountries('fr', ['FR'])
		expect(found.map((country) => country.code)).not.toContain('FR')
		expect(searchCountries('fra', [])[0]?.name).toBe('France')
	})

	it('returns nothing for a blank query and caps the results', () => {
		expect(searchCountries('  ', [])).toEqual([])
		expect(searchCountries('e', [], 5)).toHaveLength(5)
	})

	it('adds and removes a code once', () => {
		expect(withCountry(['FR'], 'BE')).toEqual(['FR', 'BE'])
		expect(withCountry(['FR'], 'FR')).toEqual(['FR'])
		expect(withoutCountry(['FR', 'BE'], 'FR')).toEqual(['BE'])
	})
})
