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
import { formatRatio, versionAgentLabel } from '#/pages/quality/lib/format'
import type { Quality } from '#/pages/quality/lib/types'

interface VersionsCardProps {
	versions: Quality['versions']
}

export function VersionsCard({ versions }: VersionsCardProps) {
	return (
		<Card aria-label="Précision par version">
			<CardHeader>
				<CardTitle>Précision par version</CardTitle>
				<CardDescription>
					Part de leads validés parmi les leads décidés, selon l'agent, le
					modèle et la version du prompt qui ont produit la qualification.
					Comparez deux versions pour savoir si un changement a aidé.
				</CardDescription>
			</CardHeader>
			<CardContent>
				{versions.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucune version enregistrée avec les décisions : les prochaines
						décisions en porteront une.
					</p>
				) : (
					<Table>
						<TableHeader>
							<TableRow>
								<TableHead>Agent</TableHead>
								<TableHead>Modèle</TableHead>
								<TableHead>Version du prompt</TableHead>
								<TableHead className="text-right">n</TableHead>
								<TableHead className="text-right">Précision</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{versions.map((row) => (
								<TableRow
									key={`${row.agent_id ?? 'aggregator'}|${row.model}|${row.prompt_version}`}
								>
									<TableCell>{versionAgentLabel(row)}</TableCell>
									<TableCell className="font-mono text-xs">
										{row.model === '' ? '—' : row.model}
									</TableCell>
									<TableCell className="font-mono text-xs">
										{row.prompt_version}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatNumber(row.n)}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatRatio(row.precision)}
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
