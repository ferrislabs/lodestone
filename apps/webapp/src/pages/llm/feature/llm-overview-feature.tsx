import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { invalidateLlm } from '#/pages/llm/feature/invalidate'
import {
	deleteConflictMessage,
	selectionFromAssignments,
	tierDiff,
} from '#/pages/llm/lib/tiers'
import type {
	Provider,
	TierAssignment,
	TierName,
	TierSelection,
} from '#/pages/llm/lib/types'
import { LlmOverview } from '#/pages/llm/ui/llm-overview'
import {
	ProvidersTable,
	type TestOutcome,
} from '#/pages/llm/ui/providers-table'
import { TierPanel } from '#/pages/llm/ui/tier-panel'

const NO_PROVIDERS: Provider[] = []
const NO_TIERS: TierAssignment[] = []

export function LlmOverviewFeature() {
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const [override, setOverride] = useState<TierSelection | null>(null)
	const [tierError, setTierError] = useState<string | null>(null)
	const [applying, setApplying] = useState(false)
	const [actionError, setActionError] = useState<string | null>(null)
	const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(
		null,
	)
	const [testOutcome, setTestOutcome] = useState<TestOutcome | null>(null)

	const providersQuery = useQuery(
		lodestoneApi().get('/api/v1/llm/providers').queryOptions,
	)
	const tiersQuery = useQuery(
		lodestoneApi().get('/api/v1/llm/tiers').queryOptions,
	)

	const providers = providersQuery.data?.data ?? NO_PROVIDERS
	const current = selectionFromAssignments(tiersQuery.data?.data ?? NO_TIERS)
	const selection = override ?? current
	const actions = tierDiff(current, selection)

	const assign = useMutation(
		lodestoneApi().mutation('put', '/api/v1/llm/tiers/{tier}').mutationOptions,
	)
	const unassign = useMutation(
		lodestoneApi().mutation('delete', '/api/v1/llm/tiers/{tier}')
			.mutationOptions,
	)
	const enable = useMutation({
		...lodestoneApi().mutation(
			'post',
			'/api/v1/llm/providers/{provider_id}/enable',
		).mutationOptions,
		onSuccess: () => invalidateLlm(queryClient),
		onError: (error) => setActionError(errorMessage(error)),
	})
	const disable = useMutation({
		...lodestoneApi().mutation(
			'post',
			'/api/v1/llm/providers/{provider_id}/disable',
		).mutationOptions,
		onSuccess: () => invalidateLlm(queryClient),
		onError: (error) => setActionError(errorMessage(error)),
	})
	const remove = useMutation(
		lodestoneApi().mutation('delete', '/api/v1/llm/providers/{provider_id}')
			.mutationOptions,
	)
	const test = useMutation(
		lodestoneApi().mutation('post', '/api/v1/llm/providers/{provider_id}/test')
			.mutationOptions,
	)

	const applyTiers = async () => {
		setApplying(true)
		setTierError(null)
		try {
			for (const action of actions) {
				if (action.op === 'assign') {
					await assign.mutateAsync({
						path: { tier: action.tier },
						body: { provider_id: action.providerId },
					})
				} else {
					await unassign.mutateAsync({ path: { tier: action.tier } })
				}
			}
			setOverride(null)
		} catch (error) {
			setTierError(parseApiError(error).message)
		} finally {
			setApplying(false)
			await invalidateLlm(queryClient)
		}
	}

	const deleteProvider = async (providerId: string) => {
		setActionError(null)
		try {
			await remove.mutateAsync({ path: { provider_id: providerId } })
			setConfirmingDeleteId(null)
		} catch (error) {
			const parsed = parseApiError(error)
			const tiers = providers.find((item) => item.id === providerId)?.tiers
			setActionError(
				parsed.status === 409
					? deleteConflictMessage(tiers ?? [])
					: parsed.message,
			)
			setConfirmingDeleteId(null)
		} finally {
			await invalidateLlm(queryClient)
		}
	}

	const testProvider = async (providerId: string) => {
		const providerName =
			providers.find((item) => item.id === providerId)?.name ?? providerId
		setTestOutcome(null)
		try {
			const response = await test.mutateAsync({
				path: { provider_id: providerId },
			})
			setTestOutcome({
				providerId,
				providerName,
				result: response.data,
				error: null,
			})
		} catch (error) {
			setTestOutcome({
				providerId,
				providerName,
				result: null,
				error: errorMessage(error),
			})
		}
	}

	const failed = providersQuery.isError
	const state = failed
		? 'error'
		: providersQuery.isPending
			? 'loading'
			: 'ready'
	const toggling = enable.isPending
		? (enable.variables?.path.provider_id ?? null)
		: disable.isPending
			? (disable.variables?.path.provider_id ?? null)
			: null

	return (
		<LlmOverview
			onCreate={() => void navigate({ to: '/llm/new' })}
			tierPanel={
				<TierPanel
					providers={providers}
					selection={selection}
					changed={actions.length > 0}
					applying={applying}
					error={
						tierError ??
						(tiersQuery.isError ? errorMessage(tiersQuery.error) : null)
					}
					onSelect={(tier: TierName, providerId: string) =>
						setOverride({ ...selection, [tier]: providerId })
					}
					onApply={() => void applyTiers()}
				/>
			}
			providers={
				<ProvidersTable
					providers={providers}
					state={state}
					errorMessage={failed ? errorMessage(providersQuery.error) : null}
					actionError={actionError}
					busyId={
						remove.isPending
							? (remove.variables?.path.provider_id ?? null)
							: toggling
					}
					testingId={
						test.isPending ? (test.variables?.path.provider_id ?? null) : null
					}
					confirmingDeleteId={confirmingDeleteId}
					testOutcome={testOutcome}
					onEdit={(providerId) =>
						void navigate({ to: '/llm/$providerId', params: { providerId } })
					}
					onToggleEnabled={(provider) => {
						setActionError(null)
						const variables = { path: { provider_id: provider.id } }
						if (provider.enabled) disable.mutate(variables)
						else enable.mutate(variables)
					}}
					onTest={(providerId) => void testProvider(providerId)}
					onRequestDelete={(providerId) => {
						setActionError(null)
						setConfirmingDeleteId(providerId)
					}}
					onConfirmDelete={(providerId) => void deleteProvider(providerId)}
					onCancelDelete={() => setConfirmingDeleteId(null)}
				/>
			}
		/>
	)
}
