import type { ProviderSummary } from '#/pages/source-providers/lib/types'

export type MatrixCell = 'native' | 'after' | 'none'

export const MATRIX_CELL_LABELS: Record<MatrixCell, string> = {
	native: 'Natif',
	after: 'Après collecte',
	none: 'Non applicable',
}

export type MatrixCriterion =
	| 'keywords'
	| 'exclusions'
	| 'remote'
	| 'countries'
	| 'contract'

export const MATRIX_ROWS: readonly {
	criterion: MatrixCriterion
	label: string
}[] = [
	{ criterion: 'keywords', label: 'Mots-clés' },
	{ criterion: 'exclusions', label: 'Exclusions' },
	{ criterion: 'remote', label: 'Télétravail' },
	{ criterion: 'countries', label: 'Pays' },
	{ criterion: 'contract', label: 'Contrat' },
]

export interface MatrixColumn {
	key: string
	name: string
	cells: Record<MatrixCriterion, MatrixCell>
}

function cellOf(
	provider: ProviderSummary,
	criterion: MatrixCriterion,
): MatrixCell {
	if (!provider.capabilities.keyword_search) return 'none'
	if (criterion === 'keywords') return 'native'
	if (criterion === 'exclusions') return 'after'
	return provider.capabilities.filters[criterion] ? 'native' : 'after'
}

export function criteriaMatrix(
	catalog: readonly ProviderSummary[],
	keys: readonly string[],
): MatrixColumn[] {
	return keys.flatMap((key) => {
		const provider = catalog.find((candidate) => candidate.key === key)
		if (provider === undefined) return []
		return [
			{
				key,
				name: provider.name,
				cells: {
					keywords: cellOf(provider, 'keywords'),
					exclusions: cellOf(provider, 'exclusions'),
					remote: cellOf(provider, 'remote'),
					countries: cellOf(provider, 'countries'),
					contract: cellOf(provider, 'contract'),
				},
			},
		]
	})
}
