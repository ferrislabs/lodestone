import { ClipboardCopyIcon } from 'lucide-react'

import { Button } from '#/components/ui/button'
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from '#/components/ui/card'
import { formatNumber } from '#/lib/format'
import {
	EXCLUSION_PROMPT_HINT,
	exclusionListName,
	exclusionOptionLabel,
} from '#/pages/quality/lib/exclusions'
import type {
	ExclusionCandidate,
	ExclusionCount,
} from '#/pages/quality/lib/types'
import { rejectionReasonLabel } from '#/pages/rejection/lib/reasons'

interface ExclusionsCardProps {
	exclusions: readonly ExclusionCount[]
	candidates: readonly ExclusionCandidate[]
	copiedLeadId: string | null
	copyFailed: boolean
	onCopy: (candidate: ExclusionCandidate) => void
}

export function ExclusionsCard({
	exclusions,
	candidates,
	copiedLeadId,
	copyFailed,
	onCopy,
}: ExclusionsCardProps) {
	return (
		<Card aria-label="Exclusions">
			<CardHeader>
				<CardTitle>Exclusions</CardTitle>
				<CardDescription>
					Leads écartés par un agent de qualification sur la période, par agent
					et par option.
				</CardDescription>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				{exclusions.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucun lead exclu par un agent sur cette période.
					</p>
				) : (
					<ul
						className="flex flex-col gap-1 text-sm"
						data-part="exclusion-counts"
					>
						{exclusions.map((row) => (
							<li
								key={`${row.agent_id}-${row.question_id}-${row.option}`}
								className="flex items-baseline justify-between gap-4"
							>
								<span>{exclusionOptionLabel(row)}</span>
								<span className="tabular-nums">{formatNumber(row.count)}</span>
							</li>
						))}
					</ul>
				)}
				{candidates.length > 0 ? (
					<section
						aria-label="Ajouter à la liste d'exclusion"
						className="flex flex-col gap-2"
					>
						<h3 className="text-sm font-medium">
							Ajouter à la liste d'exclusion
						</h3>
						<p className="text-xs text-muted-foreground">
							{EXCLUSION_PROMPT_HINT}
						</p>
						<ul className="flex flex-col gap-1.5 text-sm">
							{candidates.map((candidate) => (
								<li
									key={candidate.lead_id}
									data-candidate={candidate.lead_id}
									className="flex flex-wrap items-center justify-between gap-2"
								>
									<span className="flex flex-col">
										<span className="font-medium">
											{candidate.legal_name ?? candidate.domain}
										</span>
										<span className="text-xs text-muted-foreground">
											{candidate.legal_name ? `${candidate.domain} · ` : ''}
											{rejectionReasonLabel(candidate.reason)} · liste «{' '}
											{exclusionListName(candidate.reason)} »
										</span>
									</span>
									<Button
										type="button"
										variant="outline"
										size="sm"
										aria-label={`Copier ${candidate.legal_name ?? candidate.domain} pour la liste d'exclusion`}
										onClick={() => onCopy(candidate)}
									>
										<ClipboardCopyIcon />
										{copiedLeadId === candidate.lead_id
											? 'Copié'
											: "Ajouter à la liste d'exclusion"}
									</Button>
								</li>
							))}
						</ul>
						{copyFailed ? (
							<p role="alert" className="text-xs text-destructive">
								Impossible d'accéder au presse-papiers : copiez le nom à la
								main.
							</p>
						) : null}
					</section>
				) : null}
			</CardContent>
		</Card>
	)
}
