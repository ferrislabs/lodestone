import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { CatalogPage } from '#/pages/source-providers/ui/catalog-page'

const NO_PROVIDERS: ProviderSummary[] = []

export function CatalogFeature() {
	const navigate = useNavigate()
	const query = useQuery(
		lodestoneApi().get('/api/v1/source-providers').queryOptions,
	)
	const state = query.isError ? 'error' : query.isPending ? 'loading' : 'ready'

	return (
		<CatalogPage
			providers={query.data?.data ?? NO_PROVIDERS}
			state={state}
			errorMessage={query.isError ? errorMessage(query.error) : null}
			onOpen={(key) =>
				void navigate({ to: '/sources/providers/$key', params: { key } })
			}
			onSelectTab={(tab) => {
				if (tab === 'activity') void navigate({ to: '/sources/activity' })
			}}
		/>
	)
}
