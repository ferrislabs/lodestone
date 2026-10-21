import { describe, expect, it } from 'vitest'
import {
	DEFAULT_SORT,
	filtersFromForm,
	filtersFromSearch,
	groupingFromSearch,
	leadsSearchSchema,
	queryFromSearch,
	searchFromFilters,
} from './search'

describe('leads search params', () => {
	it('maps an empty search to default filters', () => {
		expect(filtersFromSearch({})).toEqual({
			status: '',
			verdicts: ['fiable', 'a_verifier', 'en_attente'],
			minConfidence: '',
			minScore: '',
			domain: '',
			signal: '',
			sort: DEFAULT_SORT,
			grouped: true,
		})
	})

	it('round-trips filters through the search', () => {
		const filters = {
			status: 'qualified' as const,
			verdicts: ['fiable' as const, 'ecarte' as const],
			minConfidence: '60',
			minScore: '12.5',
			domain: 'acme',
			signal: 'hiring',
			sort: 'domain_asc' as const,
			grouped: false,
		}
		const search = searchFromFilters('p1', filters)
		expect(search.profile).toBe('p1')
		expect(filtersFromSearch(search)).toEqual(filters)
	})

	it('omits empty and default values and resets the page', () => {
		const search = searchFromFilters('p1', {
			status: '',
			verdicts: ['fiable', 'a_verifier', 'en_attente'],
			minConfidence: '',
			minScore: '',
			domain: '',
			signal: '',
			sort: DEFAULT_SORT,
			grouped: true,
		})
		expect(search).toEqual({
			profile: 'p1',
			status: undefined,
			verdict: undefined,
			min_confidence: undefined,
			min_score: undefined,
			domain: undefined,
			signal: undefined,
			sort: undefined,
			group: undefined,
			page: undefined,
		})
	})

	it('groups by company by default and remembers the choice to list every lead', () => {
		expect(filtersFromSearch({}).grouped).toBe(true)
		expect(groupingFromSearch({})).toBe('company')
		const form = new FormData()
		expect(filtersFromForm(form).grouped).toBe(false)
		form.append('group', 'on')
		expect(filtersFromForm(form).grouped).toBe(true)
		const flat = searchFromFilters('p1', {
			...filtersFromSearch({}),
			grouped: false,
		})
		expect(flat.group).toBe('none')
		expect(filtersFromSearch(flat).grouped).toBe(false)
		expect(groupingFromSearch(flat)).toBe('none')
		expect(searchFromFilters('p1', filtersFromSearch({})).group).toBeUndefined()
		expect(leadsSearchSchema.safeParse({ group: 'none' }).success).toBe(true)
		expect(leadsSearchSchema.safeParse({ group: 'x' }).success).toBe(false)
	})

	it('drops an invalid min score', () => {
		const search = searchFromFilters(undefined, {
			status: '',
			verdicts: ['fiable'],
			minConfidence: '120',
			minScore: 'abc',
			domain: '',
			signal: '',
			sort: DEFAULT_SORT,
			grouped: true,
		})
		expect(search.min_score).toBeUndefined()
		expect(search.min_confidence).toBeUndefined()
		expect(search.verdict).toEqual(['fiable'])
	})

	it('hides discarded leads by default and sorts by confidence', () => {
		expect(DEFAULT_SORT).toBe('confidence_desc')
		expect(queryFromSearch({})).toEqual({
			verdict: 'fiable,a_verifier,en_attente',
			sort: 'confidence_desc',
		})
	})

	it('sends the chosen verdicts in a stable order with the min confidence', () => {
		expect(
			queryFromSearch({
				verdict: ['ecarte', 'fiable'],
				min_confidence: 40,
				sort: 'score_desc',
			}),
		).toEqual({
			verdict: 'fiable,ecarte',
			min_confidence: 40,
			sort: 'score_desc',
		})
	})

	it('sorts by the score decided by the scoring agents', () => {
		expect(queryFromSearch({ sort: 'llm_score_desc' }).sort).toBe(
			'llm_score_desc',
		)
		expect(queryFromSearch({ sort: 'llm_score_asc' }).sort).toBe(
			'llm_score_asc',
		)
	})

	it('falls back to the default verdicts when none is ticked', () => {
		const form = new FormData()
		expect(filtersFromForm(form).verdicts).toEqual([
			'fiable',
			'a_verifier',
			'en_attente',
		])
		form.append('verdict', 'ecarte')
		form.append('verdict', 'fiable')
		form.append('verdict', 'bogus')
		expect(filtersFromForm(form).verdicts).toEqual(['fiable', 'ecarte'])
	})

	it('validates the verdict and confidence search params', () => {
		expect(
			leadsSearchSchema.safeParse({ verdict: ['fiable'], min_confidence: 50 })
				.success,
		).toBe(true)
		expect(leadsSearchSchema.safeParse({ verdict: ['nope'] }).success).toBe(
			false,
		)
		expect(leadsSearchSchema.safeParse({ verdict: [] }).success).toBe(false)
		expect(leadsSearchSchema.safeParse({ min_confidence: 101 }).success).toBe(
			false,
		)
	})

	it('builds the api query with a sort and only set filters', () => {
		expect(
			queryFromSearch({
				status: 'new',
				min_score: 0,
				domain: 'x',
				sort: 'score_desc',
			}),
		).toEqual({
			status: 'new',
			verdict: 'fiable,a_verifier,en_attente',
			min_score: 0,
			domain: 'x',
			sort: 'score_desc',
		})
	})

	it('reads filters from a form', () => {
		const form = new FormData()
		form.set('status', 'rejected')
		form.set('min_confidence', ' 55 ')
		form.set('min_score', ' 3 ')
		form.set('domain', 'foo')
		form.set('signal', '')
		form.set('sort', 'bogus')
		expect(filtersFromForm(form)).toEqual({
			status: 'rejected',
			verdicts: ['fiable', 'a_verifier', 'en_attente'],
			minConfidence: '55',
			minScore: '3',
			domain: 'foo',
			signal: '',
			sort: DEFAULT_SORT,
			grouped: false,
		})
	})

	it('validates and stringifies numeric-looking text params', () => {
		const parsed = leadsSearchSchema.parse({ domain: 123, page: 2 })
		expect(parsed.domain).toBe('123')
		expect(parsed.page).toBe(2)
		expect(leadsSearchSchema.safeParse({ status: 'nope' }).success).toBe(false)
	})
})
