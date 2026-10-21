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
import { verdictKey, verdictKeyLabel } from '#/pages/leads/lib/verdict'
import { formatRatio } from '#/pages/quality/lib/format'
import type { Quality } from '#/pages/quality/lib/types'

interface VerdictPrecisionCardProps {
	verdicts: Quality['verdicts']
}

export function VerdictPrecisionCard({ verdicts }: VerdictPrecisionCardProps) {
	return (
		<Card aria-label="Précision par verdict">
			<CardHeader>
				<CardTitle>Précision par verdict</CardTitle>
				<CardDescription>
					Part de leads validés parmi les leads décidés (validés + rejetés). Les
					brouillons sont comptés à part.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>Verdict</TableHead>
							<TableHead className="text-right">Validés</TableHead>
							<TableHead className="text-right">Rejetés</TableHead>
							<TableHead className="text-right">Précision</TableHead>
							<TableHead className="text-right">n</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{verdicts.map((row) => (
							<TableRow key={row.verdict ?? 'en_attente'}>
								<TableCell>
									{verdictKeyLabel(verdictKey(row.verdict))}
								</TableCell>
								<TableCell className="text-right tabular-nums">
									{formatNumber(row.validated)}
								</TableCell>
								<TableCell className="text-right tabular-nums">
									{formatNumber(row.rejected)}
								</TableCell>
								<TableCell className="text-right tabular-nums">
									{formatRatio(row.precision)}
								</TableCell>
								<TableCell className="text-right tabular-nums">
									{formatNumber(row.n)}
								</TableCell>
							</TableRow>
						))}
					</TableBody>
				</Table>
			</CardContent>
		</Card>
	)
}
