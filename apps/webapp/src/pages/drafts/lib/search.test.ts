import { describe, expect, it } from 'vitest'

import {
	DRAFTS_PER_PAGE,
	draftsSearchSchema,
	queryFromSearch,
	searchForPage,
	searchForProfile,
	searchForVerdicts,
	verdictsFromSearch,
} from './search'

describe('drafts search params', () => {
	it('accepts an empty search', () => {
		expect(draftsSearchSchema.parse({})).toEqual({})
	})

	it('stringifies a numeric profile and validates the page', () => {
		expect(draftsSearchSchema.parse({ profile: 7, page: 2 })).toEqual({
			profile: '7',
			page: 2,
		})
		expect(draftsSearchSchema.safeParse({ page: 0 }).success).toBe(false)
	})

	it('resets the page when the profile changes', () => {
		expect(searchForProfile('p1')).toEqual({ profile: 'p1' })
		expect(searchForProfile('')).toEqual({})
	})

	it('keeps the profile and omits page 1', () => {
		expect(searchForPage({ profile: 'p1' }, 3)).toEqual({
			profile: 'p1',
			page: 3,
		})
		expect(searchForPage({ profile: 'p1', page: 3 }, 1)).toEqual({
			profile: 'p1',
		})
	})

	it('builds the api query', () => {
		expect(queryFromSearch({})).toEqual({
			verdict: 'fiable,a_verifier,en_attente',
			page: 1,
			per_page: DRAFTS_PER_PAGE,
		})
		expect(queryFromSearch({ profile: 'p1', page: 2 })).toEqual({
			profile_id: 'p1',
			verdict: 'fiable,a_verifier,en_attente',
			page: 2,
			per_page: DRAFTS_PER_PAGE,
		})
		expect(queryFromSearch({ verdict: ['ecarte', 'fiable'] }).verdict).toBe(
			'fiable,ecarte',
		)
	})

	it('keeps the verdicts when the profile or page changes', () => {
		const search = { profile: 'p1', verdict: ['ecarte' as const] }
		expect(searchForProfile('p2', search)).toEqual({
			profile: 'p2',
			verdict: ['ecarte'],
		})
		expect(searchForPage(search, 2)).toEqual({
			profile: 'p1',
			verdict: ['ecarte'],
			page: 2,
		})
	})

	it('omits the verdicts when they are the default ones and resets the page', () => {
		expect(
			searchForVerdicts({ profile: 'p1', page: 3 }, [
				'en_attente',
				'fiable',
				'a_verifier',
			]),
		).toEqual({ profile: 'p1' })
		expect(searchForVerdicts({ page: 3 }, ['ecarte'])).toEqual({
			verdict: ['ecarte'],
		})
		expect(verdictsFromSearch({})).toEqual([
			'fiable',
			'a_verifier',
			'en_attente',
		])
	})

	it('validates the verdict search param', () => {
		expect(draftsSearchSchema.safeParse({ verdict: ['nope'] }).success).toBe(
			false,
		)
		expect(draftsSearchSchema.safeParse({ verdict: ['fiable'] }).success).toBe(
			true,
		)
	})
})
