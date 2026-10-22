import { Badge } from '#/components/ui/badge'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { Verdict } from '#/pages/leads/lib/types'
import { formatConfidence } from '#/pages/leads/lib/verdict'
import { VerdictBadge } from '#/pages/leads/ui/verdict-badge'
import {
	changedText,
	decisionLabel,
	errorsText,
	formatTrialCost,
	formatTrialProgress,
	isTrialLive,
	leadChanged,
	summaryRows,
	trialStatusLabel,
	trialTitle,
} from '#/pages/profiles/lib/trials'
import type { TrialDetail } from '#/pages/profiles/lib/types'

interface TrialResultProps {
	detail: TrialDetail
	onOpenLead: (leadId: string) => void
}

interface VerdictCellProps {
	verdict: Verdict | null | undefined
	confidence: number | null | undefined
}

function VerdictCell({ verdict, confidence }: VerdictCellProps) {
	return (
		<div className="flex items-center gap-2">
			<VerdictBadge verdict={verdict} />
			<span className="font-mono text-xs text-muted-foreground">
				{formatConfidence(confidence)}
			</span>
		</div>
	)
}

export function TrialResult({ detail, onOpenLead }: TrialResultProps) {
	const { summary } = detail
	return (
		<div className="flex flex-col gap-4">
			<div className="flex flex-col gap-1">
				<div className="flex flex-wrap items-center gap-2">
					<h3 className="text-sm font-semibold">{trialTitle(detail)}</h3>
					<Badge variant="outline">{trialStatusLabel(detail.status)}</Badge>
				</div>
				<p className="text-xs text-muted-foreground">
					{formatDateTime(detail.created_at)} · {formatTrialProgress(detail)} ·{' '}
					{formatTrialCost(detail)}
				</p>
			</div>
			{detail.error ? (
				<p role="alert" className="text-sm text-destructive">
					{detail.error}
				</p>
			) : null}
			{isTrialLive(detail.status) ? (
				<output className="block text-sm text-muted-foreground">
					L'essai est en cours, les résultats s'affichent au fur et à mesure.
				</output>
			) : null}
			{detail.leads.length > 0 ? (
				<>
					<div className="flex flex-col gap-2">
						<Table aria-label="Comparaison" className="max-w-xl">
							<TableHeader>
								<TableRow>
									<TableHead>
										<span className="sr-only">Indicateur</span>
									</TableHead>
									<TableHead>Version actuelle</TableHead>
									<TableHead>Brouillon</TableHead>
								</TableRow>
							</TableHeader>
							<TableBody>
								{summaryRows(summary).map((row) => (
									<TableRow key={row.key} data-summary={row.key}>
										<TableHead scope="row">{row.label}</TableHead>
										<TableCell className="font-mono">{row.current}</TableCell>
										<TableCell className="font-mono">{row.trial}</TableCell>
									</TableRow>
								))}
							</TableBody>
						</Table>
						<p className="text-sm font-medium">
							{changedText(summary.changed)}
						</p>
						{summary.errors > 0 ? (
							<p className="text-sm text-destructive">
								{errorsText(summary.errors)}
							</p>
						) : null}
					</div>
					<Table aria-label="Leads de l'essai">
						<TableHeader>
							<TableRow>
								<TableHead>Domaine</TableHead>
								<TableHead>Décision</TableHead>
								<TableHead>Verdict actuel</TableHead>
								<TableHead>Verdict du brouillon</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{detail.leads.map((lead) => (
								<TableRow
									key={lead.lead_id}
									data-trial-lead={lead.lead_id}
									data-changed={leadChanged(lead) || undefined}
									className="data-[changed]:bg-amber-50"
								>
									<TableCell>
										<a
											href={`/leads/${lead.lead_id}`}
											className="font-medium hover:underline"
											onClick={(event) => {
												event.preventDefault()
												onOpenLead(lead.lead_id)
											}}
										>
											{lead.domain}
										</a>
									</TableCell>
									<TableCell>{decisionLabel(lead.label)}</TableCell>
									<TableCell>
										<VerdictCell
											verdict={lead.current_verdict}
											confidence={lead.current_confidence}
										/>
									</TableCell>
									<TableCell>
										{lead.error ? (
											<span
												data-part="error"
												className="text-xs text-destructive"
											>
												{lead.error}
											</span>
										) : (
											<VerdictCell
												verdict={lead.trial_verdict}
												confidence={lead.trial_confidence}
											/>
										)}
									</TableCell>
								</TableRow>
							))}
						</TableBody>
					</Table>
				</>
			) : (
				<p className="text-sm text-muted-foreground">
					Aucun lead essayé pour l'instant.
				</p>
			)}
		</div>
	)
}
