import { z } from 'zod'

export const PERIODS = ['7', '30', '90', 'all'] as const

export type Period = (typeof PERIODS)[number]

export const DEFAULT_PERIOD: Period = '30'

export const PERIOD_LABELS: Readonly<Record<Period, string>> = {
	'7': '7 j',
	'30': '30 j',
	'90': '90 j',
	all: 'Tout',
}

const DAY_MS = 86_400_000

const text = z.union([z.string(), z.number()]).transform(String)

export const qualitySearchSchema = z.object({
	period: z.enum(PERIODS).optional().catch(undefined),
	profile: text.optional(),
})

export type QualitySearch = z.output<typeof qualitySearchSchema>

export function periodOf(search: QualitySearch): Period {
	return search.period ?? DEFAULT_PERIOD
}

export function periodStart(period: Period, now: number): string | undefined {
	if (period === 'all') return undefined
	return new Date(now - Number(period) * DAY_MS).toISOString()
}

export function searchFor(period: Period, profile: string): QualitySearch {
	return {
		...(period === DEFAULT_PERIOD ? {} : { period }),
		...(profile === '' ? {} : { profile }),
	}
}

export function queryFor(
	search: QualitySearch,
	now: number,
): { from?: string; profile_id?: string } {
	const from = periodStart(periodOf(search), now)
	return {
		...(from === undefined ? {} : { from }),
		...(search.profile ? { profile_id: search.profile } : {}),
	}
}
