import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import {
	filterRounds,
	presentSteps,
	type TimelineStep,
	timelinePollMs,
	toggleStep,
} from '#/pages/leads/lib/timeline'

export function useLeadTimeline(leadId: string) {
	const [selected, setSelected] = useState<TimelineStep[]>([])
	const query = useQuery({
		...lodestoneApi().get('/api/v1/leads/{lead_id}/timeline', {
			path: { lead_id: leadId },
		}).queryOptions,
		refetchInterval: (current) =>
			timelinePollMs(current.state.data?.data, Date.now()),
	})
	const timeline = query.data?.data ?? null
	const rounds = timeline?.rounds ?? []
	return {
		state: query.isError
			? ('error' as const)
			: query.isPending
				? ('loading' as const)
				: ('ready' as const),
		error: query.isError ? errorMessage(query.error) : null,
		timeline,
		rounds: filterRounds(rounds, selected),
		steps: presentSteps(rounds),
		selected,
		now: query.dataUpdatedAt || Date.now(),
		toggle: (step: TimelineStep) =>
			setSelected((steps) => toggleStep(steps, step)),
		clear: () => setSelected([]),
	}
}
