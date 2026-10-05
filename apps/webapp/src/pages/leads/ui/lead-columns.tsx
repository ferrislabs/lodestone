import type { ColumnDef } from '@tanstack/react-table'

import { formatDateTime } from '#/pages/leads/lib/format'
import type { Lead } from '#/pages/leads/lib/types'
import { LeadStatusBadge } from './lead-status-badge'
import { ScoreBar } from './score-bar'

export function buildLeadColumns(
	maxScore: number,
	onOpenLead: (leadId: string) => void,
): ColumnDef<Lead>[] {
	return [
		{
			id: 'domain',
			header: 'Domain',
			cell: ({ row }) => (
				<button
					type="button"
					className="font-medium text-foreground hover:underline"
					onClick={() => onOpenLead(row.original.id)}
				>
					{row.original.domain}
				</button>
			),
		},
		{
			id: 'score',
			header: 'Score',
			cell: ({ row }) => <ScoreBar score={row.original.score} max={maxScore} />,
		},
		{
			id: 'status',
			header: 'Status',
			cell: ({ row }) => <LeadStatusBadge status={row.original.status} />,
		},
		{
			id: 'last_seen',
			header: 'Last seen',
			cell: ({ row }) => (
				<span className="text-muted-foreground">
					{formatDateTime(row.original.last_seen_at)}
				</span>
			),
		},
	]
}
