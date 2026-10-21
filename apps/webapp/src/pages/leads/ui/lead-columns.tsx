import type { ColumnDef } from '@tanstack/react-table'
import { groupedBadge } from '#/pages/leads/lib/company'
import {
	confidenceBreakdown,
	exclusionSummary,
	exclusionTitle,
} from '#/pages/leads/lib/confidence'
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
			cell: ({ row }) => {
				const badge = groupedBadge(row.original.grouped_leads)
				return (
					<div className="flex flex-col items-start gap-1">
						<button
							type="button"
							className="font-medium text-foreground hover:underline"
							onClick={() => onOpenLead(row.original.id)}
						>
							{row.original.domain}
						</button>
						{badge ? (
							<button
								type="button"
								data-group-toggle={row.original.id}
								aria-expanded={row.getIsExpanded()}
								className="rounded-md border bg-muted px-2 py-0.5 text-xs text-muted-foreground hover:bg-accent"
								onClick={row.getToggleExpandedHandler()}
							>
								{badge}
							</button>
						) : null}
					</div>
				)
			},
		},
		{
			id: 'verdict',
			header: 'Verdict',
			cell: ({ row }) => {
				const exclusions = row.original.exclusions
				const summary = exclusionSummary(exclusions)
				return (
					<div
						className="flex flex-col items-start gap-1"
						title={exclusions.map(exclusionTitle).join('\n') || undefined}
					>
						<VerdictBadge verdict={row.original.verdict} />
						{summary ? (
							<span
								className="text-xs text-muted-foreground"
								data-part="exclusion"
							>
								{summary}
							</span>
						) : null}
					</div>
				)
			},
		},
		{
			id: 'confidence',
			header: 'Confiance',
			cell: ({ row }) => (
				<ConfidenceBar
					confidence={row.original.confidence}
					breakdown={confidenceBreakdown(row.original)}
				/>
			),
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
