import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { useSourceTrigger } from '#/pages/sources/feature/use-source-trigger'
import {
	activeTriggers,
	POLL_INTERVAL_MS,
	shouldPoll,
} from '#/pages/sources/lib/state'
import type { Source } from '#/pages/sources/lib/types'
import { SourcesPage } from '#/pages/sources/ui/sources-page'

const NO_SOURCES: Source[] = []

export function SourcesFeature() {
	const navigate = useNavigate()
	const { trigger, results, triggeredAt, pending } = useSourceTrigger()
	const query = useQuery({
		...lodestoneApi().get('/api/v1/sources').queryOptions,
		refetchInterval: (current) =>
			shouldPoll(
				current.state.data?.data ?? NO_SOURCES,
				triggeredAt,
				Date.now(),
			)
				? POLL_INTERVAL_MS
				: false,
	})
	const sources = query.data?.data ?? NO_SOURCES
	const now = Date.now()
	const errors: Record<string, string> = {}
	for (const [name, result] of Object.entries(results)) {
		if (result.kind === 'running') errors[name] = 'La source est déjà en cours'
		if (result.kind === 'error') errors[name] = result.message
	}
	const state = query.isError ? 'error' : query.isPending ? 'loading' : 'ready'

	return (
		<SourcesPage
			sources={sources}
			state={state}
			errorMessage={query.isError ? errorMessage(query.error) : null}
			now={now}
			scheduled={activeTriggers(sources, triggeredAt, now)}
			pending={pending}
			errors={errors}
			onRun={trigger}
			onOpenProvider={(key) =>
				void navigate({ to: '/sources/providers/$key', params: { key } })
			}
		/>
	)
}
