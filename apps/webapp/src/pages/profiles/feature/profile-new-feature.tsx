import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useState } from 'react'

import { lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { defaultValue, toJsonObject } from '#/lib/schema-form'
import {
	renderDefinitionForm,
	unmatchedProblems,
} from '#/pages/profiles/feature/definition-form'
import { invalidateProfiles } from '#/pages/profiles/feature/invalidate'
import { useDefinitionEditor } from '#/pages/profiles/feature/use-definition-editor'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import {
	problemsFromError,
	problemsOutsidePath,
} from '#/pages/profiles/lib/problems'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import { prepareSave } from '#/pages/profiles/lib/save'
import type { Problem } from '#/pages/profiles/lib/types'
import { ProfileEditor } from '#/pages/profiles/ui/profile-editor'
import { ProfileNew } from '#/pages/profiles/ui/profile-new'

export function ProfileNewFeature() {
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const [name, setName] = useState('')
	const [initial] = useState(() =>
		toJsonObject(defaultValue(PROFILE_SCHEMA, PROFILE_SCHEMA, PROFILE_HINTS)),
	)
	const editor = useDefinitionEditor(initial)
	const [localProblems, setLocalProblems] = useState<Problem[]>([])

	const create = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/profiles').mutationOptions,
		onSuccess: async (response) => {
			await invalidateProfiles(queryClient)
			void navigate({
				to: '/profiles/$profileId',
				params: { profileId: response.data.id },
			})
		},
	})

	const parsed = create.isError ? parseApiError(create.error) : null
	const serverProblems =
		parsed?.status === 422 ? problemsFromError(create.error) : []
	const problems = [
		...localProblems,
		...problemsOutsidePath(serverProblems, 'name'),
	]
	const nameError = parsed?.fields.name ?? null
	const formError = parsed && parsed.status !== 422 ? parsed.message : null

	const save = () => {
		const definition = prepareSave(editor.value)
		if (!definition.ok) {
			setLocalProblems(definition.problems)
			return
		}
		setLocalProblems([])
		const trimmed = name.trim()
		create.mutate({
			body:
				trimmed === ''
					? { definition: definition.value }
					: { name: trimmed, definition: definition.value },
		})
	}

	return (
		<ProfileNew
			name={name}
			nameError={nameError}
			formError={formError}
			onNameChange={(value) => {
				if (create.isError) create.reset()
				setName(value)
			}}
			onBack={() => void navigate({ to: '/profiles' })}
			editor={
				<ProfileEditor
					form={renderDefinitionForm(editor.value, problems, (next) => {
						if (create.isError) create.reset()
						setLocalProblems([])
						editor.set(next)
					})}
					problems={unmatchedProblems(editor.value, problems)}
					notice={null}
					dirty
					saving={create.isPending}
					saveLabel="Create profile"
					onSave={save}
				/>
			}
		/>
	)
}
