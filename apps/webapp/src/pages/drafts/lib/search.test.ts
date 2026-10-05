import { describe, expect, it } from 'vitest'

import {
	DRAFTS_PER_PAGE,
	draftsSearchSchema,
	queryFromSearch,
	searchForPage,
	searchForProfile,
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
		expect(queryFromSearch({})).toEqual({ page: 1, per_page: DRAFTS_PER_PAGE })
		expect(queryFromSearch({ profile: 'p1', page: 2 })).toEqual({
			profile_id: 'p1',
			page: 2,
			per_page: DRAFTS_PER_PAGE,
		})
	})
})
