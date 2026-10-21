import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'

import { lodestoneApi } from '#/api/lodestone'
import type { ProviderOption } from '#/pages/profiles/lib/qualification'

export function useProviderOptions(): ProviderOption[] {
	const query = useQuery(
		lodestoneApi().get('/api/v1/llm/providers').queryOptions,
	)
	const providers = query.data?.data
	return useMemo(
		() =>
			(providers ?? []).map((provider) => ({
				id: provider.id,
				name: provider.name,
				enabled: provider.enabled,
			})),
		[providers],
	)
}
