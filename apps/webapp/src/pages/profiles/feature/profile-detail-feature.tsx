import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { toJsonObject } from '#/lib/schema-form'
import {
	DefinitionTabContent,
	unmatchedProblems,
} from '#/pages/profiles/feature/definition-form'
import { invalidateProfiles } from '#/pages/profiles/feature/invalidate'
import { downloadProfileExport } from '#/pages/profiles/feature/profile-export'
import { useAgentCalibration } from '#/pages/profiles/feature/use-agent-calibration'
import { useDefinitionEditor } from '#/pages/profiles/feature/use-definition-editor'
import { useEcosystemUi } from '#/pages/profiles/feature/use-ecosystem-ui'
import { useProfileActivation } from '#/pages/profiles/feature/use-profile-activation'
import { useProfileTrials } from '#/pages/profiles/feature/use-profile-trials'
import { useProviderOptions } from '#/pages/profiles/feature/use-provider-options'
import { useSourceCatalog } from '#/pages/profiles/feature/use-source-catalog'
import { useTrialLauncher } from '#/pages/profiles/feature/use-trial-launcher'
import {
	prettyDefinition,
	saveResultMessage,
} from '#/pages/profiles/lib/definition'
import { problemsFromError } from '#/pages/profiles/lib/problems'
import { prepareSave } from '#/pages/profiles/lib/save'
import {
	editorTabItems,
	isEditorTab,
	type ProfileTab,
	TAB_LABELS,
	type TabItem,
} from '#/pages/profiles/lib/tabs'
import { parseTrialLimits } from '#/pages/profiles/lib/trials'
import type {
	Problem,
	RevisionSummary,
	Trial,
} from '#/pages/profiles/lib/types'
import { ProfileActivation } from '#/pages/profiles/ui/profile-activation'
import { ProfileDetail } from '#/pages/profiles/ui/profile-detail'
import { ProfileEditor } from '#/pages/profiles/ui/profile-editor'
import { RevisionHistory } from '#/pages/profiles/ui/revision-history'
import { TrialLauncher } from '#/pages/profiles/ui/trial-launcher'
import { TrialsPanel } from '#/pages/profiles/ui/trials-panel'
import { useSourceTrigger } from '#/pages/sources/feature/use-source-trigger'
import { listenerSummaries } from '#/pages/sources/lib/listener'

const NO_REVISIONS: RevisionSummary[] = []
const NO_TRIALS: Trial[] = []
const TRIAL_REFUSED =
	'Le brouillon est refusé : corrigez les problèmes signalés.'
const LISTENER_REFRESH_MS = 15_000

export function ProfileDetailFeature() {
	const { profileId } = useParams({ from: '/_app/profiles/$profileId' })
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const activation = useProfileActivation()
	const sourceTrigger = useSourceTrigger()
	const providers = useProviderOptions()
	const catalog = useSourceCatalog()
	const calibration = useAgentCalibration(profileId)
	const ecosystem = useEcosystemUi(profileId)
	const sourcesQuery = useQuery({
		...lodestoneApi().get('/api/v1/sources').queryOptions,
		refetchInterval: LISTENER_REFRESH_MS,
	})
	const listeners = listenerSummaries(sourcesQuery.data?.data ?? [])
	const search = useSearch({ from: '/_app/profiles/$profileId' })
	const tab = search.tab ?? 'identity'
	const setTab = (next: ProfileTab) =>
		void navigate({
			to: '/profiles/$profileId',
			params: { profileId },
			search: { tab: next },
			replace: true,
		})
	const setTrial = (trial: string) =>
		void navigate({
			to: '/profiles/$profileId',
			params: { profileId },
			search: { tab: 'trials', trial },
			replace: true,
		})
	const setAgent = (agent: string | null, replace: boolean) =>
		void navigate({
			to: '/profiles/$profileId',
			params: { profileId },
			search: agent === null ? { tab: 'sourcing' } : { tab: 'sourcing', agent },
			replace,
		})
	const [localProblems, setLocalProblems] = useState<Problem[]>([])
	const [notice, setNotice] = useState<string | null>(null)
	const [selected, setSelected] = useState<number | null>(null)
	const [exporting, setExporting] = useState(false)
	const [exportError, setExportError] = useState<string | null>(null)
	const launcher = useTrialLauncher()
	const trialId = search.trial ?? null
	const profileTrials = useProfileTrials(profileId, trialId, tab === 'trials')

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
			launcher.close()
			setNotice(saveResultMessage(profile?.revision ?? 0, response.data.number))
			await invalidateProfiles(queryClient)
		},
	})

	const startFailure = profileTrials.start.isError
		? parseApiError(profileTrials.start.error)
		: null
	const startRefused = startFailure?.status === 422
	const problems = [
		...localProblems,
		...(update.isError ? problemsFromError(update.error) : []),
		...(startRefused ? problemsFromError(profileTrials.start.error) : []),
	]
	const launcherMessage =
		launcher.message ??
		(startRefused ? TRIAL_REFUSED : (startFailure?.message ?? null))

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

	const requestTrial = () => {
		setNotice(null)
		profileTrials.start.reset()
		const parsed = prepareSave(editor.value)
		if (!parsed.ok) {
			setLocalProblems(parsed.problems)
			launcher.close()
			return
		}
		setLocalProblems([])
		launcher.show()
	}

	const launchTrial = () => {
		const parsed = prepareSave(editor.value)
		if (!parsed.ok) {
			setLocalProblems(parsed.problems)
			launcher.close()
			return
		}
		setLocalProblems([])
		const limits = parseTrialLimits(launcher.sampleSize, launcher.budgetCents)
		if (!limits.ok) {
			launcher.fail(limits.message)
			return
		}
		profileTrials.start.mutate(
			{
				path: { profile_id: profileId },
				body: {
					definition: parsed.value,
					sample_size: limits.sampleSize,
					budget_cents: limits.budgetCents,
				},
			},
			{
				onSuccess: (response) => {
					launcher.close()
					setTrial(response.data.id)
				},
			},
		)
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
		{ id: 'trials', label: TAB_LABELS.trials, problems: 0 },
	]

	const definition = (
		<ProfileEditor
			form={
				isEditorTab(tab) ? (
					<DefinitionTabContent
						tab={tab}
						value={editor.value}
						problems={problems}
						sourceTrigger={sourceTrigger}
						listeners={listeners}
						providers={providers}
						catalog={catalog}
						onOpenProvider={(key) =>
							void navigate({ to: '/sources/providers/$key', params: { key } })
						}
						calibration={calibration}
						ecosystem={ecosystem}
						agentId={search.agent ?? null}
						onAgentChange={setAgent}
						now={Date.now()}
						onChange={(next) => {
							if (update.isError) update.reset()
							if (profileTrials.start.isError) profileTrials.start.reset()
							setLocalProblems([])
							setNotice(null)
							editor.set(next)
						}}
					/>
				) : null
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
				profileTrials.start.reset()
				launcher.close()
				setLocalProblems([])
				setNotice(null)
				editor.reset()
			}}
			trialPanel={
				launcher.open ? (
					<TrialLauncher
						sampleSize={launcher.sampleSize}
						budgetCents={launcher.budgetCents}
						busy={profileTrials.start.isPending}
						message={launcherMessage}
						onSampleSize={launcher.setSampleSize}
						onBudgetCents={launcher.setBudgetCents}
						onLaunch={launchTrial}
						onCancel={launcher.close}
					/>
				) : null
			}
			onTry={requestTrial}
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

	const trialsPanel = (
		<TrialsPanel
			trials={profileTrials.list.data?.data ?? NO_TRIALS}
			loading={profileTrials.list.isPending}
			error={
				profileTrials.list.isError
					? errorMessage(profileTrials.list.error)
					: null
			}
			selected={trialId}
			detail={profileTrials.detail.data?.data ?? null}
			loadingDetail={trialId !== null && profileTrials.detail.isPending}
			detailError={
				profileTrials.detail.isError
					? errorMessage(profileTrials.detail.error)
					: null
			}
			onSelect={setTrial}
			onOpenLead={(leadId) =>
				void navigate({ to: '/leads/$leadId', params: { leadId } })
			}
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
			trials={trialsPanel}
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
