import { z } from 'zod'

import {
	DEFAULT_VERDICTS,
	isDefaultVerdicts,
	orderedVerdicts,
	VERDICT_KEYS,
	type VerdictKey,
} from '#/pages/leads/lib/verdict'

export const DRAFTS_PER_PAGE = 20

const text = z.union([z.string(), z.number()]).transform(String)

export const draftsSearchSchema = z.object({
	profile: text.optional(),
	verdict: z
		.array(z.enum(VERDICT_KEYS as [VerdictKey, ...VerdictKey[]]))
		.min(1)
		.optional(),
	page: z.number().int().min(1).optional(),
})

export type DraftsSearch = z.output<typeof draftsSearchSchema>

export function verdictsFromSearch(search: DraftsSearch): VerdictKey[] {
	return orderedVerdicts(search.verdict ?? DEFAULT_VERDICTS)
}

export function searchForProfile(
	profile: string,
	search: DraftsSearch = {},
): DraftsSearch {
	return {
		...(profile === '' ? {} : { profile }),
		...(search.verdict ? { verdict: search.verdict } : {}),
	}
}

export function searchForVerdicts(
	search: DraftsSearch,
	verdicts: readonly VerdictKey[],
): DraftsSearch {
	return {
		...(search.profile ? { profile: search.profile } : {}),
		...(verdicts.length === 0 || isDefaultVerdicts(verdicts)
			? {}
			: { verdict: orderedVerdicts(verdicts) }),
	}
}

export function searchForPage(
	search: DraftsSearch,
	page: number,
): DraftsSearch {
	return {
		...(search.profile ? { profile: search.profile } : {}),
		...(search.verdict ? { verdict: search.verdict } : {}),
		...(page > 1 ? { page } : {}),
	}
}

export function queryFromSearch(search: DraftsSearch): {
	profile_id?: string
	verdict: string
	page: number
	per_page: number
} {
	return {
		...(search.profile ? { profile_id: search.profile } : {}),
		verdict: verdictsFromSearch(search).join(','),
		page: search.page ?? 1,
		per_page: DRAFTS_PER_PAGE,
	}
}
