import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { lodestoneApi } from '#/api/lodestone'
import { invalidateProfiles } from '#/pages/profiles/feature/invalidate'
import { isTrialLive, TRIAL_POLL_MS } from '#/pages/profiles/lib/trials'

export function useProfileTrials(
	profileId: string,
	trialId: string | null,
	active: boolean,
) {
	const queryClient = useQueryClient()
	const list = useQuery({
		...lodestoneApi().get('/api/v1/profiles/{profile_id}/trials', {
			path: { profile_id: profileId },
		}).queryOptions,
		enabled: active,
		refetchInterval: (query) =>
			query.state.data?.data.some((trial) => isTrialLive(trial.status))
				? TRIAL_POLL_MS
				: false,
	})
	const detail = useQuery({
		...lodestoneApi().get('/api/v1/trials/{trial_id}', {
			path: { trial_id: trialId ?? '' },
		}).queryOptions,
		enabled: active && trialId !== null,
		refetchInterval: (query) =>
			query.state.data && isTrialLive(query.state.data.data.status)
				? TRIAL_POLL_MS
				: false,
	})
	const start = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/profiles/{profile_id}/trials')
			.mutationOptions,
		onSuccess: () => invalidateProfiles(queryClient),
	})
	return { list, detail, start }
}
