import { useQuery } from '@tanstack/react-query'

import { lodestoneApi } from '#/api/lodestone'

export function usePendingDraftsCount(): number | undefined {
	const query = useQuery(
		lodestoneApi().get('/api/v1/drafts', {
			query: { page: 1, per_page: 1 },
		}).queryOptions,
	)
	return query.data?.pagination?.total ?? undefined
}
