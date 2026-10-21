import type { ColumnDef } from '@tanstack/react-table'

import { formatDateTime } from '#/pages/leads/lib/format'
import type { Lead } from '#/pages/leads/lib/types'
import { formatConfidence } from '#/pages/leads/lib/verdict'
import { LeadStatusBadge } from './lead-status-badge'
import { ScoreBar } from './score-bar'
import { VerdictBadge } from './verdict-badge'

export function buildLeadColumns(
	maxScore: number,
	onOpenLead: (leadId: string) => void,
): ColumnDef<Lead>[] {
	return [
		{
			id: 'domain',
			header: 'Domaine',
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
			id: 'verdict',
			header: 'Verdict',
			cell: ({ row }) => (
				<div className="flex items-center gap-2">
					<VerdictBadge verdict={row.original.verdict} />
					<span className="font-mono text-xs tabular-nums text-muted-foreground">
						{formatConfidence(row.original.confidence)}
					</span>
				</div>
			),
		},
		{
			id: 'status',
			header: 'Statut',
			cell: ({ row }) => <LeadStatusBadge status={row.original.status} />,
		},
		{
			id: 'last_seen',
			header: 'Dernière détection',
			cell: ({ row }) => (
				<span className="text-muted-foreground">
					{formatDateTime(row.original.last_seen_at)}
				</span>
			),
		},
	]
}
