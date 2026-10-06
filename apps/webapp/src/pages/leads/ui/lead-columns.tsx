import type { ColumnDef } from '@tanstack/react-table'

import { formatDateTime } from '#/pages/leads/lib/format'
import type { Lead } from '#/pages/leads/lib/types'
import { summaryExcerpt } from '#/pages/leads/lib/verdict'
import { ConfidenceBar } from './confidence-bar'
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
			id: 'verdict',
			header: 'Verdict',
			cell: ({ row }) => <VerdictBadge verdict={row.original.verdict} />,
		},
		{
			id: 'confidence',
			header: 'Confiance',
			cell: ({ row }) => <ConfidenceBar confidence={row.original.confidence} />,
		},
		{
			id: 'summary',
			header: 'Synthèse',
			cell: ({ row }) => {
				const excerpt = summaryExcerpt(row.original.summary)
				return excerpt === '' ? (
					<span className="text-muted-foreground">Pas encore de synthèse</span>
				) : (
					<span
						className="block max-w-md text-sm"
						title={row.original.summary ?? undefined}
					>
						{excerpt}
					</span>
				)
			},
		},
		{
			id: 'score',
			header: 'Score',
			cell: ({ row }) => <ScoreBar score={row.original.score} max={maxScore} />,
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
