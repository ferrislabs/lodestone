import { z } from 'zod'

import { LEAD_STATUSES } from './transitions'
import type { LeadStatus } from './types'

export const PER_PAGE = 25

export const LEAD_SORTS = [
	'score_desc',
	'score_asc',
	'last_seen_desc',
	'first_seen_desc',
	'domain_asc',
] as const

export type LeadSort = (typeof LEAD_SORTS)[number]

export const DEFAULT_SORT: LeadSort = 'score_desc'

const text = z.union([z.string(), z.number()]).transform(String)

export const leadsSearchSchema = z.object({
	profile: text.optional(),
	status: z.enum(LEAD_STATUSES as [LeadStatus, ...LeadStatus[]]).optional(),
	min_score: z.number().min(0).optional(),
	domain: text.optional(),
	signal: text.optional(),
	sort: z.enum(LEAD_SORTS).optional(),
	page: z.number().int().min(1).optional(),
})

export type LeadsSearch = z.output<typeof leadsSearchSchema>

export interface LeadFilters {
	status: LeadStatus | ''
	minScore: string
	domain: string
	signal: string
	sort: LeadSort
}

export interface LeadsQuery {
	status?: LeadStatus
	min_score?: number
	domain?: string
	signal?: string
	sort: LeadSort
}

export function filtersFromSearch(search: LeadsSearch): LeadFilters {
	return {
		status: search.status ?? '',
		minScore: search.min_score === undefined ? '' : String(search.min_score),
		domain: search.domain ?? '',
		signal: search.signal ?? '',
		sort: search.sort ?? DEFAULT_SORT,
	}
}

function parseStatus(raw: string): LeadStatus | undefined {
	return LEAD_STATUSES.find((status) => status === raw)
}

function parseSort(raw: string): LeadSort {
	return LEAD_SORTS.find((sort) => sort === raw) ?? DEFAULT_SORT
}

export function filtersFromForm(form: FormData): LeadFilters {
	const read = (name: string): string => {
		const value = form.get(name)
		return typeof value === 'string' ? value.trim() : ''
	}
	return {
		status: parseStatus(read('status')) ?? '',
		minScore: read('min_score'),
		domain: read('domain'),
		signal: read('signal'),
		sort: parseSort(read('sort')),
	}
}

export function searchFromFilters(
	profile: string | undefined,
	filters: LeadFilters,
): LeadsSearch {
	const minScore = Number(filters.minScore)
	const hasMinScore =
		filters.minScore !== '' && Number.isFinite(minScore) && minScore >= 0
	return {
		profile,
		status: filters.status === '' ? undefined : filters.status,
		min_score: hasMinScore ? minScore : undefined,
		domain: filters.domain === '' ? undefined : filters.domain,
		signal: filters.signal === '' ? undefined : filters.signal,
		sort: filters.sort === DEFAULT_SORT ? undefined : filters.sort,
		page: undefined,
	}
}

export function queryFromSearch(search: LeadsSearch): LeadsQuery {
	return {
		...(search.status ? { status: search.status } : {}),
		...(search.min_score === undefined ? {} : { min_score: search.min_score }),
		...(search.domain ? { domain: search.domain } : {}),
		...(search.signal ? { signal: search.signal } : {}),
		sort: search.sort ?? DEFAULT_SORT,
	}
}
