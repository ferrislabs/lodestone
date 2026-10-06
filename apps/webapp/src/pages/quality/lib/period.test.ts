import { describe, expect, it } from 'vitest'

import {
	periodOf,
	periodStart,
	qualitySearchSchema,
	queryFor,
	searchFor,
} from './period'

const NOW = Date.parse('2026-10-06T12:00:00Z')

describe('periodStart', () => {
	it('goes back by the chosen number of days', () => {
		expect(periodStart('7', NOW)).toBe('2026-09-29T12:00:00.000Z')
		expect(periodStart('30', NOW)).toBe('2026-09-06T12:00:00.000Z')
		expect(periodStart('90', NOW)).toBe('2026-07-08T12:00:00.000Z')
	})

	it('has no lower bound for the whole history', () => {
		expect(periodStart('all', NOW)).toBeUndefined()
	})
})

describe('search', () => {
	it('defaults to thirty days and ignores unknown periods', () => {
		expect(periodOf(qualitySearchSchema.parse({}))).toBe('30')
		expect(periodOf(qualitySearchSchema.parse({ period: 'forever' }))).toBe(
			'30',
		)
		expect(periodOf(qualitySearchSchema.parse({ period: '90' }))).toBe('90')
	})

	it('keeps only what differs from the defaults', () => {
		expect(searchFor('30', '')).toEqual({})
		expect(searchFor('7', 'p1')).toEqual({ period: '7', profile: 'p1' })
	})

	it('builds the query from the search', () => {
		expect(queryFor({}, NOW)).toEqual({ from: '2026-09-06T12:00:00.000Z' })
		expect(queryFor({ period: 'all', profile: 'p1' }, NOW)).toEqual({
			profile_id: 'p1',
		})
	})
})
