import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { useCallback, useMemo, useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import {
	filtersFromSearch,
	groupingFromSearch,
	type LeadFilters,
	PER_PAGE,
	queryFromSearch,
	searchFromFilters,
} from '#/pages/leads/lib/search'
import type { Lead } from '#/pages/leads/lib/types'
import { buildLeadColumns } from '#/pages/leads/ui/lead-columns'
import { LeadFiltersForm } from '#/pages/leads/ui/lead-filters'
import { LeadsList } from '#/pages/leads/ui/leads-list'
import { downloadLeadExport, type ExportFormat } from './lead-export'

const NO_LEADS: Lead[] = []

export function LeadsListFeature() {
	const search = useSearch({ from: '/_app/leads/' })
	const navigate = useNavigate()
	const [exporting, setExporting] = useState(false)
	const [exportError, setExportError] = useState<string | null>(null)

	const profilesQuery = useQuery(
		lodestoneApi().get('/api/v1/profiles').queryOptions,
	)
	const profiles = profilesQuery.data?.data ?? []
	const profileId = search.profile ?? profiles[0]?.id
	const query = queryFromSearch(search)

	const leadsQuery = useQuery({
		...lodestoneApi().get('/api/v1/profiles/{profile_id}/leads', {
			path: { profile_id: profileId ?? '' },
			query: {
				...query,
				group_by: groupingFromSearch(search),
				page: search.page ?? 1,
				per_page: PER_PAGE,
			},
		}).queryOptions,
		enabled: profileId !== undefined,
		placeholderData: keepPreviousData,
	})

	const leads = leadsQuery.data?.data ?? NO_LEADS
	const maxScore = leads.reduce((max, lead) => Math.max(max, lead.score), 0)

	const openLead = useCallback(
		(leadId: string) => {
			void navigate({ to: '/leads/$leadId', params: { leadId } })
		},
		[navigate],
	)

	const columns = useMemo(
		() => buildLeadColumns(maxScore, openLead),
		[maxScore, openLead],
	)

	const table = useReactTable({
		data: leads,
		columns,
		getCoreRowModel: getCoreRowModel(),
		getRowCanExpand: (row) => row.original.grouped_leads.length > 0,
		manualPagination: true,
		manualSorting: true,
		getRowId: (lead) => lead.id,
	})

	const goTo = (next: typeof search) => {
		void navigate({ to: '/leads', search: next })
	}

	const applyFilters = (filters: LeadFilters) => {
		goTo(searchFromFilters(profileId, filters))
	}

	const handleExport = (format: ExportFormat) => {
		if (profileId === undefined) return
		setExporting(true)
		setExportError(null)
		downloadLeadExport(profileId, format, query)
			.catch((error: unknown) => setExportError(errorMessage(error)))
			.finally(() => setExporting(false))
	}

	const failed = profilesQuery.isError || leadsQuery.isError
	const loading =
		profilesQuery.isPending || (profileId !== undefined && leadsQuery.isPending)
	const state = failed ? 'error' : loading ? 'loading' : 'ready'

	return (
		<LeadsList
			toolbar={
				<LeadFiltersForm
					profiles={profiles}
					profileId={profileId}
					filters={filtersFromSearch(search)}
					onProfileChange={(next) => goTo({ profile: next })}
					onApply={applyFilters}
					onReset={() => goTo({ profile: profileId })}
				/>
			}
			table={table}
			state={state}
			errorMessage={
				profilesQuery.isError
					? errorMessage(profilesQuery.error)
					: leadsQuery.isError
						? errorMessage(leadsQuery.error)
						: null
			}
			pagination={leadsQuery.data?.pagination ?? null}
			onPageChange={(page) => goTo({ ...search, profile: profileId, page })}
			onExport={handleExport}
			exporting={exporting}
			exportError={exportError}
			columnCount={columns.length}
			onOpenLead={openLead}
			noProfile={profileId === undefined}
		/>
	)
}
