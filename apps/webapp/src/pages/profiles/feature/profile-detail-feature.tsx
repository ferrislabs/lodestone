import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { toJsonObject } from '#/lib/schema-form'
import {
	DefinitionTabContent,
	unmatchedProblems,
} from '#/pages/profiles/feature/definition-form'
import { invalidateProfiles } from '#/pages/profiles/feature/invalidate'
import { downloadProfileExport } from '#/pages/profiles/feature/profile-export'
import { useDefinitionEditor } from '#/pages/profiles/feature/use-definition-editor'
import { useProfileActivation } from '#/pages/profiles/feature/use-profile-activation'
import {
	prettyDefinition,
	saveResultMessage,
} from '#/pages/profiles/lib/definition'
import { problemsFromError } from '#/pages/profiles/lib/problems'
import { prepareSave } from '#/pages/profiles/lib/save'
import {
	editorTabItems,
	type ProfileTab,
	TAB_LABELS,
	type TabItem,
} from '#/pages/profiles/lib/tabs'
import type { Problem, RevisionSummary } from '#/pages/profiles/lib/types'
import { ProfileActivation } from '#/pages/profiles/ui/profile-activation'
import { ProfileDetail } from '#/pages/profiles/ui/profile-detail'
import { ProfileEditor } from '#/pages/profiles/ui/profile-editor'
import { RevisionHistory } from '#/pages/profiles/ui/revision-history'

const NO_REVISIONS: RevisionSummary[] = []

export function ProfileDetailFeature() {
	const { profileId } = useParams({ from: '/_app/profiles/$profileId' })
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const activation = useProfileActivation()
	const search = useSearch({ from: '/_app/profiles/$profileId' })
	const tab = search.tab ?? 'identity'
	const setTab = (next: ProfileTab) =>
		void navigate({
			to: '/profiles/$profileId',
			params: { profileId },
			search: { tab: next },
			replace: true,
		})
	const [localProblems, setLocalProblems] = useState<Problem[]>([])
	const [notice, setNotice] = useState<string | null>(null)
	const [selected, setSelected] = useState<number | null>(null)
	const [exporting, setExporting] = useState(false)
	const [exportError, setExportError] = useState<string | null>(null)

	const profileQuery = useQuery(
		lodestoneApi().get('/api/v1/profiles/{profile_id}', {
			path: { profile_id: profileId },
		}).queryOptions,
	)
	const revisionsQuery = useQuery(
		lodestoneApi().get('/api/v1/profiles/{profile_id}/revisions', {
			path: { profile_id: profileId },
		}).queryOptions,
	)
	const revisionQuery = useQuery({
		...lodestoneApi().get('/api/v1/profiles/{profile_id}/revisions/{number}', {
			path: { profile_id: profileId, number: selected ?? 1 },
		}).queryOptions,
		enabled: selected !== null,
	})

	const profile = profileQuery.data?.data ?? null
	const initial = useMemo(
		() => toJsonObject(profile?.definition ?? {}),
		[profile?.definition],
	)
	const editor = useDefinitionEditor(initial)

	const update = useMutation({
		...lodestoneApi().mutation('put', '/api/v1/profiles/{profile_id}')
			.mutationOptions,
		onSuccess: async (response) => {
			editor.set(toJsonObject(response.data.definition))
			setNotice(saveResultMessage(profile?.revision ?? 0, response.data.number))
			await invalidateProfiles(queryClient)
		},
	})

	const problems = [
		...localProblems,
		...(update.isError ? problemsFromError(update.error) : []),
	]

	const save = () => {
		setNotice(null)
		const parsed = prepareSave(editor.value)
		if (!parsed.ok) {
			setLocalProblems(parsed.problems)
			return
		}
		setLocalProblems([])
		update.mutate({
			path: { profile_id: profileId },
			body: { definition: parsed.value },
		})
	}

	const revision = revisionQuery.data?.data
	const state = profileQuery.isError
		? 'error'
		: profileQuery.isPending
			? 'loading'
			: 'ready'

	const tabItems: TabItem[] = [
		...editorTabItems(problems),
		{ id: 'history', label: TAB_LABELS.history, problems: 0 },
	]

	const definition = (
		<ProfileEditor
			form={
				tab === 'history' ? null : (
					<DefinitionTabContent
						tab={tab}
						value={editor.value}
						problems={problems}
						onNavigate={setTab}
						onChange={(next) => {
							if (update.isError) update.reset()
							setLocalProblems([])
							setNotice(null)
							editor.set(next)
						}}
					/>
				)
			}
			problems={unmatchedProblems(editor.value, problems)}
			notice={notice}
			dirty={editor.dirty}
			saving={update.isPending}
			saveLabel="Enregistrer comme nouvelle révision"
			confirmingReset={editor.confirmingReset}
			onRequestReset={editor.requestReset}
			onCancelReset={editor.cancelReset}
			onConfirmReset={() => {
				if (update.isError) update.reset()
				setLocalProblems([])
				setNotice(null)
				editor.reset()
			}}
			onSave={save}
		/>
	)

	const history = (
		<RevisionHistory
			revisions={revisionsQuery.data?.data ?? NO_REVISIONS}
			selected={selected}
			selectedText={revision ? prettyDefinition(revision.definition) : null}
			loadingSelected={selected !== null && revisionQuery.isPending}
			selectedError={
				revisionQuery.isError ? errorMessage(revisionQuery.error) : null
			}
			onSelect={setSelected}
			onRestore={() => {
				if (!revision) return
				update.reset()
				setLocalProblems([])
				setNotice(
					`Révision ${revision.number} chargée, enregistrer pour l'appliquer`,
				)
				editor.set(toJsonObject(revision.definition))
				setTab('identity')
			}}
		/>
	)

	return (
		<ProfileDetail
			state={state}
			errorMessage={
				profileQuery.isError ? errorMessage(profileQuery.error) : null
			}
			profile={profile}
			activation={
				profile ? (
					<ProfileActivation
						active={profile.active}
						confirming={activation.confirmingId === profile.id}
						busy={activation.busyId === profile.id}
						onActivate={() => activation.activate(profile.id)}
						onRequestDeactivate={() => activation.requestDeactivate(profile.id)}
						onConfirmDeactivate={() => activation.confirmDeactivate(profile.id)}
						onCancelDeactivate={activation.cancelDeactivate}
					/>
				) : null
			}
			tab={tab}
			tabs={tabItems}
			exporting={exporting}
			exportError={exportError ?? activation.error}
			definition={definition}
			history={history}
			onTabChange={setTab}
			onExport={() => {
				if (!profile) return
				setExporting(true)
				setExportError(null)
				downloadProfileExport(profile.id, profile.name)
					.catch((error: unknown) => setExportError(errorMessage(error)))
					.finally(() => setExporting(false))
			}}
			onBack={() => void navigate({ to: '/profiles' })}
		/>
	)
}
