import { DnsSummary } from '#/components/ecosystem/dns-summary'
import { FindingList } from '#/components/ecosystem/finding-list'
import { HiringSummary } from '#/components/ecosystem/hiring-summary'
import { RecentItems } from '#/components/ecosystem/recent-items'
import { TechGroups } from '#/components/ecosystem/tech-groups'
import type { ModuleView } from '#/lib/ecosystem-views'

interface ModuleFindingsProps {
	view: ModuleView
	empty: string
}

function isEmpty(view: ModuleView): boolean {
	if (view.rows.length > 0) return false
	switch (view.kind) {
		case 'website':
			return view.groups.length === 0
		case 'dns':
			return view.lines.length === 0
		case 'hiring':
			return view.summary === null
		case 'recent':
			return view.recent === null
		case 'list':
			return true
	}
}

export function ModuleFindings({ view, empty }: ModuleFindingsProps) {
	if (isEmpty(view)) {
		return <p className="text-sm text-muted-foreground">{empty}</p>
	}
	return (
		<div className="flex flex-col gap-3">
			{view.kind === 'website' && view.groups.length > 0 ? (
				<TechGroups groups={view.groups} />
			) : null}
			{view.kind === 'dns' ? <DnsSummary lines={view.lines} /> : null}
			{view.kind === 'hiring' && view.summary ? (
				<HiringSummary summary={view.summary} />
			) : null}
			{view.kind === 'recent' && view.recent ? (
				<RecentItems recent={view.recent} />
			) : null}
			{view.rows.length > 0 ? (
				<FindingList rows={view.rows} empty={empty} />
			) : null}
		</div>
	)
}
