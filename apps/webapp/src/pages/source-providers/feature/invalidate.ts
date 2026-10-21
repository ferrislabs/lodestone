import type { QueryClient } from '@tanstack/react-query'

export async function invalidateSourceProviders(
	queryClient: QueryClient,
): Promise<void> {
	await queryClient.invalidateQueries({
		predicate: (query) => {
			const key = query.queryKey[0]
			if (typeof key !== 'object' || key === null || !('_id' in key)) {
				return false
			}
			const id = key._id
			return typeof id === 'string' && id.startsWith('/api/v1/source-providers')
		},
	})
}
