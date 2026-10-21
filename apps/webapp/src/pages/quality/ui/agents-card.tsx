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
import { formatScore, formatSeparation } from '#/pages/quality/lib/format'
import type { Quality } from '#/pages/quality/lib/types'

interface AgentsCardProps {
	agents: Quality['agents']
}

export function AgentsCard({ agents }: AgentsCardProps) {
	return (
		<Card aria-label="Agents">
			<CardHeader>
				<CardTitle>Agents</CardTitle>
				<CardDescription>
					Score moyen donné aux leads validés et rejetés. Plus l'écart est
					grand, mieux l'agent départage les leads.
				</CardDescription>
			</CardHeader>
			<CardContent>
				{agents.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucun score d'agent enregistré avec les décisions.
					</p>
				) : (
					<Table>
						<TableHeader>
							<TableRow>
								<TableHead>Agent</TableHead>
								<TableHead className="text-right">
									Score moyen validés
								</TableHead>
								<TableHead className="text-right">
									Score moyen rejetés
								</TableHead>
								<TableHead className="text-right">Écart</TableHead>
								<TableHead className="text-right">n validés</TableHead>
								<TableHead className="text-right">n rejetés</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{agents.map((agent) => (
								<TableRow key={agent.agent_id}>
									<TableCell>
										<div>{agent.name}</div>
										{agent.name !== agent.agent_id ? (
											<div className="font-mono text-xs text-muted-foreground">
												{agent.agent_id}
											</div>
										) : null}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatScore(agent.mean_validated)}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatScore(agent.mean_rejected)}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatSeparation(agent.separation)}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{agent.validated}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{agent.rejected}
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
