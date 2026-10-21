import { Badge } from '#/components/ui/badge'
import { cn } from '#/lib/utils'
import {
	enrichmentRows,
	enrichmentStatusLabel,
	type StepState,
	waitingBadge,
} from '#/pages/leads/lib/enrichment'
import type { Enrichment } from '#/pages/leads/lib/types'

const STATE_STYLES: Readonly<Record<StepState, string>> = {
	done: 'border-emerald-200 bg-emerald-50 text-emerald-700',
	pending: 'border-amber-200 bg-amber-50 text-amber-700',
	disabled: 'border-slate-200 bg-slate-50 text-slate-500',
}

interface LeadEnrichmentProps {
	enrichment: Enrichment
}

export function LeadEnrichment({ enrichment }: LeadEnrichmentProps) {
	const rows = enrichmentRows(enrichment)
	const waiting = waitingBadge(enrichment)
	return (
		<section
			data-section="enrichment"
			aria-label="Enrichissement"
			className="flex flex-col gap-2 rounded-lg border p-4"
		>
			<div className="flex flex-wrap items-center gap-3">
				<h2 className="text-sm font-semibold">Enrichissement</h2>
				<span className="text-xs text-muted-foreground">
					{enrichmentStatusLabel(enrichment.status)}
				</span>
				{waiting ? (
					<Badge
						variant="outline"
						data-waiting={waiting.reason}
						className="border-amber-200 bg-amber-50 text-amber-700"
					>
						{waiting.detail
							? `${waiting.label} ${waiting.detail}`
							: waiting.label}
					</Badge>
				) : null}
			</div>
			{rows.length > 0 ? (
				<ul className="flex flex-col gap-1 text-sm">
					{rows.map((row) => (
						<li
							key={row.step}
							data-step={row.step}
							data-state={row.state}
							className="flex flex-wrap items-center gap-2"
						>
							<span>{row.label}</span>
							<Badge variant="outline" className={cn(STATE_STYLES[row.state])}>
								{row.stateLabel}
							</Badge>
							{row.date ? (
								<span className="text-xs text-muted-foreground">
									{row.date}
								</span>
							) : null}
						</li>
					))}
				</ul>
			) : null}
		</section>
	)
}
