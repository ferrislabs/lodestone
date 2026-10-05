import { describe, expect, it } from 'vitest'
import {
	DEFAULT_SORT,
	filtersFromForm,
	filtersFromSearch,
	leadsSearchSchema,
	queryFromSearch,
	searchFromFilters,
} from './search'

describe('leads search params', () => {
	it('maps an empty search to default filters', () => {
		expect(filtersFromSearch({})).toEqual({
			status: '',
			minScore: '',
			domain: '',
			signal: '',
			sort: DEFAULT_SORT,
		})
	})

	it('round-trips filters through the search', () => {
		const filters = {
			status: 'qualified' as const,
			minScore: '12.5',
			domain: 'acme',
			signal: 'hiring',
			sort: 'domain_asc' as const,
		}
		const search = searchFromFilters('p1', filters)
		expect(search.profile).toBe('p1')
		expect(filtersFromSearch(search)).toEqual(filters)
	})

	it('omits empty and default values and resets the page', () => {
		const search = searchFromFilters('p1', {
			status: '',
			minScore: '',
			domain: '',
			signal: '',
			sort: DEFAULT_SORT,
		})
		expect(search).toEqual({
			profile: 'p1',
			status: undefined,
			min_score: undefined,
			domain: undefined,
			signal: undefined,
			sort: undefined,
			page: undefined,
		})
	})

	it('drops an invalid min score', () => {
		const search = searchFromFilters(undefined, {
			status: '',
			minScore: 'abc',
			domain: '',
			signal: '',
			sort: DEFAULT_SORT,
		})
		expect(search.min_score).toBeUndefined()
	})

	it('builds the api query with a sort and only set filters', () => {
		expect(
			queryFromSearch({ status: 'new', min_score: 0, domain: 'x' }),
		).toEqual({ status: 'new', min_score: 0, domain: 'x', sort: 'score_desc' })
	})

	it('reads filters from a form', () => {
		const form = new FormData()
		form.set('status', 'rejected')
		form.set('min_score', ' 3 ')
		form.set('domain', 'foo')
		form.set('signal', '')
		form.set('sort', 'bogus')
		expect(filtersFromForm(form)).toEqual({
			status: 'rejected',
			minScore: '3',
			domain: 'foo',
			signal: '',
			sort: DEFAULT_SORT,
		})
	})

	it('validates and stringifies numeric-looking text params', () => {
		const parsed = leadsSearchSchema.parse({ domain: 123, page: 2 })
		expect(parsed.domain).toBe('123')
		expect(parsed.page).toBe(2)
		expect(leadsSearchSchema.safeParse({ status: 'nope' }).success).toBe(false)
	})
})
