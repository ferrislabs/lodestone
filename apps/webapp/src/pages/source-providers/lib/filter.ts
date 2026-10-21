import {
	capabilityLabels,
	categoryLabel,
	groupByCategory,
} from '#/pages/source-providers/lib/labels'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'

export interface CatalogFilter {
	category: string | null
	status: string | null
	query: string
}

export interface CategoryCount {
	category: string
	title: string
	count: number
}

export function normalizeText(text: string): string {
	return text
		.normalize('NFD')
		.replace(/\p{Diacritic}/gu, '')
		.toLowerCase()
		.trim()
}

export function matchesQuery(
	provider: ProviderSummary,
	query: string,
): boolean {
	const needle = normalizeText(query)
	if (needle === '') return true
	const haystack = normalizeText(
		[
			provider.name,
			provider.description,
			provider.key,
			...capabilityLabels(provider),
		].join(' '),
	)
	return needle.split(/\s+/).every((word) => haystack.includes(word))
}

export function filterProviders(
	providers: readonly ProviderSummary[],
	filter: CatalogFilter,
): ProviderSummary[] {
	const matching = providers.filter(
		(provider) =>
			(filter.category === null || provider.category === filter.category) &&
			(filter.status === null || provider.status === filter.status) &&
			matchesQuery(provider, filter.query),
	)
	return groupByCategory(matching).flatMap((group) => group.providers)
}

export function categoryCounts(
	providers: readonly ProviderSummary[],
	query: string,
): CategoryCount[] {
	return groupByCategory(providers).map((group) => ({
		category: group.category,
		title: categoryLabel(group.category),
		count: group.providers.filter((provider) => matchesQuery(provider, query))
			.length,
	}))
}
