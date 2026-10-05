import { z } from 'zod'

export const DRAFTS_PER_PAGE = 20

const text = z.union([z.string(), z.number()]).transform(String)

export const draftsSearchSchema = z.object({
	profile: text.optional(),
	page: z.number().int().min(1).optional(),
})

export type DraftsSearch = z.output<typeof draftsSearchSchema>

export function searchForProfile(profile: string): DraftsSearch {
	return profile === '' ? {} : { profile }
}

export function searchForPage(
	search: DraftsSearch,
	page: number,
): DraftsSearch {
	return {
		...(search.profile ? { profile: search.profile } : {}),
		...(page > 1 ? { page } : {}),
	}
}

export function queryFromSearch(search: DraftsSearch): {
	profile_id?: string
	page: number
	per_page: number
} {
	return {
		...(search.profile ? { profile_id: search.profile } : {}),
		page: search.page ?? 1,
		per_page: DRAFTS_PER_PAGE,
	}
}
