import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useState } from 'react'

import { lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { defaultValue, toJsonObject } from '#/lib/schema-form'
import {
	DefinitionTabContent,
	unmatchedProblems,
} from '#/pages/profiles/feature/definition-form'
import { invalidateProfiles } from '#/pages/profiles/feature/invalidate'
import { useDefinitionEditor } from '#/pages/profiles/feature/use-definition-editor'
import { useProviderOptions } from '#/pages/profiles/feature/use-provider-options'
import { agentList } from '#/pages/profiles/lib/agents'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import {
	problemsFromError,
	problemsOutsidePath,
} from '#/pages/profiles/lib/problems'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import {
	newQualificationAgent,
	qualificationList,
	withQualification,
} from '#/pages/profiles/lib/qualification'
import { prepareSave } from '#/pages/profiles/lib/save'
import { asEditorTab, editorTabItems } from '#/pages/profiles/lib/tabs'
import type { Problem } from '#/pages/profiles/lib/types'
import { ProfileEditor } from '#/pages/profiles/ui/profile-editor'
import { ProfileNew } from '#/pages/profiles/ui/profile-new'
import { TabBar } from '#/pages/profiles/ui/tab-bar'

export function ProfileNewFeature() {
	const navigate = useNavigate()
	const search = useSearch({ from: '/_app/profiles/new' })
	const tab = asEditorTab(search.tab)
	const queryClient = useQueryClient()
	const providers = useProviderOptions()
	const [name, setName] = useState('')
	const [initial] = useState(() =>
		withQualification(
			toJsonObject(defaultValue(PROFILE_SCHEMA, PROFILE_SCHEMA, PROFILE_HINTS)),
			[newQualificationAgent('pertinence', new Set())],
		),
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

	const noAgents = agentList(editor.value).length === 0
	const noQualification = qualificationList(editor.value).length === 0

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
			tabs={
				<TabBar
					tabs={editorTabItems(problems)}
					active={tab}
					onChange={(next) =>
						void navigate({
							to: '/profiles/new',
							search: { tab: next },
							replace: true,
						})
					}
				/>
			}
			editor={
				<ProfileEditor
					form={
						<DefinitionTabContent
							tab={tab}
							value={editor.value}
							problems={problems}
							providers={providers}
							onChange={(next) => {
								if (create.isError) create.reset()
								setLocalProblems([])
								editor.set(next)
							}}
						/>
					}
					blockedReason={
						noAgents
							? 'Ajouter au moins un agent pour créer le profil.'
							: noQualification
								? 'Ajouter au moins un agent de qualification pour créer le profil.'
								: null
					}
					problems={unmatchedProblems(editor.value, problems)}
					notice={null}
					dirty={editor.dirty}
					saving={create.isPending}
					saveLabel="Créer le profil"
					confirmingReset={editor.confirmingReset}
					onRequestReset={editor.requestReset}
					onCancelReset={editor.cancelReset}
					onConfirmReset={() => {
						if (create.isError) create.reset()
						setLocalProblems([])
						editor.reset()
					}}
					onSave={save}
				/>
			}
		/>
	)
}
