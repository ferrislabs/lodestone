import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { invalidateProfiles } from '#/pages/profiles/feature/invalidate'
import { useProfileActivation } from '#/pages/profiles/feature/use-profile-activation'
import {
	parseDefinition,
	problemsFromError,
} from '#/pages/profiles/lib/problems'
import type { Problem, ProfileSummary } from '#/pages/profiles/lib/types'
import { ProfilesList } from '#/pages/profiles/ui/profiles-list'

const NO_PROFILES: ProfileSummary[] = []

export function ProfilesListFeature() {
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const activation = useProfileActivation()
	const [importName, setImportName] = useState('')
	const [importProblems, setImportProblems] = useState<Problem[]>([])
	const [importError, setImportError] = useState<string | null>(null)

	const profilesQuery = useQuery(
		lodestoneApi().get('/api/v1/profiles').queryOptions,
	)

	const importProfile = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/profiles/import')
			.mutationOptions,
		onSuccess: async (response) => {
			setImportName('')
			await invalidateProfiles(queryClient)
			void navigate({
				to: '/profiles/$profileId',
				params: { profileId: response.data.id },
			})
		},
		onError: (error) => {
			const parsed = parseApiError(error)
			if (parsed.status === 422) {
				setImportProblems(problemsFromError(error))
			} else {
				setImportError(parsed.message)
			}
		},
	})

	const onImportFile = async (file: File) => {
		setImportProblems([])
		setImportError(null)
		const parsed = parseDefinition(await file.text())
		if (!parsed.ok) {
			setImportProblems(parsed.problems)
			return
		}
		const name = importName.trim()
		importProfile.mutate({
			query: name === '' ? {} : { name },
			body: parsed.value,
		})
	}

	const failed = profilesQuery.isError
	const state = failed ? 'error' : profilesQuery.isPending ? 'loading' : 'ready'

	return (
		<ProfilesList
			profiles={profilesQuery.data?.data ?? NO_PROFILES}
			state={state}
			errorMessage={failed ? errorMessage(profilesQuery.error) : null}
			actionError={activation.error}
			busyId={activation.busyId}
			confirmingId={activation.confirmingId}
			importName={importName}
			importing={importProfile.isPending}
			importProblems={importProblems}
			importError={importError}
			onImportNameChange={setImportName}
			onImportFile={(file) => void onImportFile(file)}
			onCreate={() => void navigate({ to: '/profiles/new' })}
			onOpen={(profileId) =>
				void navigate({ to: '/profiles/$profileId', params: { profileId } })
			}
			onActivate={activation.activate}
			onRequestDeactivate={activation.requestDeactivate}
			onConfirmDeactivate={activation.confirmDeactivate}
			onCancelDeactivate={activation.cancelDeactivate}
		/>
	)
}
