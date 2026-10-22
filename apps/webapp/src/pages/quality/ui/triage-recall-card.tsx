import { Button } from '#/components/ui/button'
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from '#/components/ui/card'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import { formatNumber } from '#/lib/format'
import { agentLabel, dropReasonLabel } from '#/pages/quality/lib/dropped'
import { formatRatio } from '#/pages/quality/lib/format'
import type { ProfileSummary, TriageRecall } from '#/pages/quality/lib/types'

interface TriageRecallCardProps {
	rows: readonly TriageRecall[]
	profiles: readonly ProfileSummary[]
	onReview: (row: TriageRecall) => void
}

function profileName(
	profiles: readonly ProfileSummary[],
	profileId: string,
): string | null {
	return profiles.find((profile) => profile.id === profileId)?.name ?? null
}

export function TriageRecallCard({
	rows,
	profiles,
	onReview,
}: TriageRecallCardProps) {
	return (
		<Card aria-label="Rappel du tri">
			<CardHeader>
				<CardTitle>Rappel du tri</CardTitle>
				<CardDescription>
					Annonces écartées par le tri par mots-clés sur la période, et part de
					celles qu'un opérateur a jugées à garder.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>Agent</TableHead>
							<TableHead>Raison</TableHead>
							<TableHead className="text-right">Écartées</TableHead>
							<TableHead className="text-right">Revues</TableHead>
							<TableHead className="text-right">À garder</TableHead>
							<TableHead className="text-right">Part</TableHead>
							<TableHead>
								<span className="sr-only">Actions</span>
							</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{rows.map((row) => {
							const name = profileName(profiles, row.profile_id)
							return (
								<TableRow
									key={`${row.profile_id}-${row.agent ?? ''}-${row.reason}`}
								>
									<TableCell>
										<div>{agentLabel(row.agent)}</div>
										{name === null ? null : (
											<div className="text-xs text-muted-foreground">
												{name}
											</div>
										)}
									</TableCell>
									<TableCell>{dropReasonLabel(row.reason)}</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatNumber(row.dropped)}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatNumber(row.reviewed)}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatNumber(row.should_have_kept)}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatRatio(row.share)}
									</TableCell>
									<TableCell className="text-right">
										<Button
											type="button"
											variant="link"
											size="sm"
											onClick={() => onReview(row)}
										>
											Revoir un échantillon
										</Button>
									</TableCell>
								</TableRow>
							)
						})}
					</TableBody>
				</Table>
			</CardContent>
		</Card>
	)
}
