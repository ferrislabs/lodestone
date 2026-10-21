import { useQuery } from '@tanstack/react-query'

import { lodestoneApi } from '#/api/lodestone'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'

const NO_PROVIDERS: ProviderSummary[] = []

export function useSourceCatalog(): ProviderSummary[] {
	const query = useQuery(
		lodestoneApi().get('/api/v1/source-providers').queryOptions,
	)
	return query.data?.data ?? NO_PROVIDERS
}
