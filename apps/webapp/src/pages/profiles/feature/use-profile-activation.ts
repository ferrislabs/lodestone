import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { invalidateProfiles } from '#/pages/profiles/feature/invalidate'

export function useProfileActivation() {
	const queryClient = useQueryClient()
	const [confirmingId, setConfirmingId] = useState<string | null>(null)

	const activate = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/profiles/{profile_id}/activate')
			.mutationOptions,
		onSuccess: () => invalidateProfiles(queryClient),
	})
	const deactivate = useMutation({
		...lodestoneApi().mutation(
			'post',
			'/api/v1/profiles/{profile_id}/deactivate',
		).mutationOptions,
		onSuccess: async () => {
			setConfirmingId(null)
			await invalidateProfiles(queryClient)
		},
	})

	const failure = activate.isError
		? activate.error
		: deactivate.isError
			? deactivate.error
			: null

	return {
		confirmingId,
		busyId: activate.isPending
			? (activate.variables?.path.profile_id ?? null)
			: deactivate.isPending
				? (deactivate.variables?.path.profile_id ?? null)
				: null,
		error: failure ? errorMessage(failure) : null,
		activate: (profileId: string) => {
			deactivate.reset()
			activate.mutate({ path: { profile_id: profileId } })
		},
		requestDeactivate: (profileId: string) => setConfirmingId(profileId),
		cancelDeactivate: () => setConfirmingId(null),
		confirmDeactivate: (profileId: string) => {
			activate.reset()
			deactivate.mutate({ path: { profile_id: profileId } })
		},
	}
}
