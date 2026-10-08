import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { CrawlRulesSheet } from '#/pages/source-providers/ui/crawl-rules-panel'

const CRAWL_SETTINGS_PATH = '/api/v1/settings/crawl'

interface CrawlRulesFeatureProps {
	open: boolean
	onOpenChange: (open: boolean) => void
}

export function CrawlRulesFeature({
	open,
	onOpenChange,
}: CrawlRulesFeatureProps) {
	const queryClient = useQueryClient()
	const [confirming, setConfirming] = useState(false)
	const [retried, setRetried] = useState<number | null>(null)
	const query = useQuery({
		...lodestoneApi().get(CRAWL_SETTINGS_PATH).queryOptions,
		enabled: open,
	})
	const save = useMutation({
		...lodestoneApi().mutation('put', CRAWL_SETTINGS_PATH).mutationOptions,
		gcTime: 0,
		onSuccess: async (response) => {
			setConfirming(false)
			setRetried(response.data.dossiers_retried)
			await queryClient.invalidateQueries({
				predicate: (cached) => {
					const key = cached.queryKey[0]
					return (
						typeof key === 'object' &&
						key !== null &&
						'_id' in key &&
						key._id === CRAWL_SETTINGS_PATH
					)
				},
			})
		},
		onError: () => setConfirming(false),
	})
	const state = query.isError ? 'error' : query.isPending ? 'loading' : 'ready'
	const saveAs = (robots_policy: 'respect' | 'ignore') =>
		save.mutate({ body: { robots_policy } })

	return (
		<CrawlRulesSheet
			open={open}
			onOpenChange={onOpenChange}
			settings={query.data?.data ?? null}
			state={state}
			errorMessage={query.isError ? errorMessage(query.error) : null}
			saving={save.isPending}
			saveError={save.isError ? errorMessage(save.error) : null}
			retried={retried}
			confirming={confirming}
			onToggle={(ignore) => {
				if (ignore) {
					setConfirming(true)
				} else {
					saveAs('respect')
				}
			}}
			onConfirm={() => saveAs('ignore')}
			onCancel={() => setConfirming(false)}
		/>
	)
}
