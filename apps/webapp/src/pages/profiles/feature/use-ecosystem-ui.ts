import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import type { Schemas } from '#/api/api.client'
import { errorMessage, lodestoneApi } from '#/api/lodestone'
import {
	type DryRunView,
	dryRunView,
	type EcosystemModuleInfo,
} from '#/pages/profiles/lib/enrichment'
import type {
	EcosystemTestActions,
	EcosystemTestState,
} from '#/pages/profiles/ui/ecosystem-test-panel'
import type { ModulesState } from '#/pages/profiles/ui/enrichment-editor'

const LEAD_OPTIONS = 8
const NO_MODULES: readonly EcosystemModuleInfo[] = []

interface Testing {
	module: Schemas.EcosystemModule
	query: string
	selectedId: string | null
}

export interface EcosystemUi {
	modules: readonly EcosystemModuleInfo[]
	modulesState: ModulesState
	test: EcosystemTestState | null
	onTest: (module: string) => void
	testActions: EcosystemTestActions
}

export function useEcosystemUi(profileId: string | null): EcosystemUi {
	const [testing, setTesting] = useState<Testing | null>(null)
	const [result, setResult] = useState<DryRunView | null>(null)
	const modulesQuery = useQuery(
		lodestoneApi().get('/api/v1/ecosystem/modules').queryOptions,
	)
	const leadsQuery = useQuery({
		...lodestoneApi().get('/api/v1/profiles/{profile_id}/leads', {
			path: { profile_id: profileId ?? '' },
			query: {
				per_page: LEAD_OPTIONS,
				...(testing && testing.query.trim() !== ''
					? { domain: testing.query.trim() }
					: {}),
			},
		}).queryOptions,
		enabled: testing !== null && profileId !== null,
	})
	const dryRun = useMutation({
		...lodestoneApi().mutation(
			'post',
			'/api/v1/leads/{lead_id}/ecosystem/dry-run',
		).mutationOptions,
		onSuccess: (response) => setResult(dryRunView(response.data)),
	})
	const modules = modulesQuery.data?.data ?? NO_MODULES
	const modulesState: ModulesState = modulesQuery.isError
		? 'error'
		: modulesQuery.isPending
			? 'loading'
			: 'ready'
	const label =
		modules.find((module) => module.module === testing?.module)?.label ?? ''
	const test: EcosystemTestState | null =
		testing === null
			? null
			: {
					module: testing.module,
					moduleLabel: label,
					query: testing.query,
					options: (leadsQuery.data?.data ?? []).map((lead) => ({
						id: lead.id,
						domain: lead.domain,
					})),
					searching: leadsQuery.isFetching,
					selectedId: testing.selectedId,
					running: dryRun.isPending,
					result,
					error: dryRun.isError ? errorMessage(dryRun.error) : null,
					unavailableReason:
						profileId === null
							? 'Enregistrez le profil pour tester un module sur un de ses leads.'
							: null,
				}
	const onTest = (module: string) => {
		const found = modules.find((known) => known.module === module)
		if (!found) return
		dryRun.reset()
		setResult(null)
		setTesting({ module: found.module, query: '', selectedId: null })
	}
	return {
		modules,
		modulesState,
		test,
		onTest,
		testActions: {
			onClose: () => {
				dryRun.reset()
				setResult(null)
				setTesting(null)
			},
			onQuery: (query) =>
				setTesting((current) => (current ? { ...current, query } : current)),
			onPick: (leadId) =>
				setTesting((current) =>
					current ? { ...current, selectedId: leadId } : current,
				),
			onRun: () => {
				if (testing === null || testing.selectedId === null) return
				setResult(null)
				dryRun.mutate({
					path: { lead_id: testing.selectedId },
					query: { module: testing.module },
				})
			},
		},
	}
}
