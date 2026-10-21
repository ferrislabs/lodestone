import { useQuery } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { CrawlRulesFeature } from '#/pages/source-providers/feature/crawl-rules-feature'
import type { CatalogSearch } from '#/pages/source-providers/lib/search'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { CatalogPage } from '#/pages/source-providers/ui/catalog-page'

const NO_PROVIDERS: ProviderSummary[] = []

export function CatalogFeature() {
	const navigate = useNavigate()
	const search = useSearch({ from: '/_app/sources/' })
	const setSearch = (next: CatalogSearch) =>
		void navigate({ to: '/sources', search: next, replace: true })
	const query = useQuery(
		lodestoneApi().get('/api/v1/source-providers').queryOptions,
	)
	const state = query.isError ? 'error' : query.isPending ? 'loading' : 'ready'
	const [rulesOpen, setRulesOpen] = useState(false)

	return (
		<>
			<CatalogPage
				providers={query.data?.data ?? NO_PROVIDERS}
				state={state}
				errorMessage={query.isError ? errorMessage(query.error) : null}
				category={search.category ?? null}
				query={search.q ?? ''}
				status={search.status ?? null}
				onCategoryChange={(category) =>
					setSearch({
						q: search.q,
						status: search.status,
						category: category ?? undefined,
					})
				}
				onStatusChange={(status) =>
					setSearch({
						q: search.q,
						category: search.category,
						status: status ?? undefined,
					})
				}
				onQueryChange={(q) =>
					setSearch({
						category: search.category,
						status: search.status,
						q: q === '' ? undefined : q,
					})
				}
				onReset={() => setSearch({})}
				onOpen={(key) =>
					void navigate({ to: '/sources/providers/$key', params: { key } })
				}
				onSelectTab={(tab) => {
					if (tab === 'activity') void navigate({ to: '/sources/activity' })
				}}
				onOpenRules={() => setRulesOpen(true)}
			/>
			<CrawlRulesFeature open={rulesOpen} onOpenChange={setRulesOpen} />
		</>
	)
}
