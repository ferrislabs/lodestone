import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import { formatScore } from '#/pages/leads/lib/format'
import type { ScoreContribution } from '#/pages/leads/lib/types'

interface ScoreBreakdownProps {
	contributions: readonly ScoreContribution[]
}

export function ScoreBreakdown({ contributions }: ScoreBreakdownProps) {
	return (
		<Card>
			<CardHeader>
				<CardTitle>Score breakdown</CardTitle>
			</CardHeader>
			<CardContent>
				{contributions.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						No score contribution.
					</p>
				) : (
					<Table>
						<TableHeader>
							<TableRow>
								<TableHead>Signal</TableHead>
								<TableHead>Value</TableHead>
								<TableHead className="text-right">Weight</TableHead>
								<TableHead className="text-right">Contribution</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{contributions.map((item) => (
								<TableRow
									key={`${item.signal}-${item.value ?? ''}-${item.contribution}`}
								>
									<TableCell className="font-medium">{item.signal}</TableCell>
									<TableCell>{item.value ?? '-'}</TableCell>
									<TableCell className="text-right font-mono">
										{formatScore(item.weight)}
									</TableCell>
									<TableCell className="text-right font-mono">
										{formatScore(item.contribution)}
									</TableCell>
								</TableRow>
							))}
						</TableBody>
					</Table>
				)}
			</CardContent>
		</Card>
	)
}
