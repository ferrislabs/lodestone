import { ChevronDownIcon, ChevronRightIcon } from 'lucide-react'
import { Fragment } from 'react'

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
import { formatRatio } from '#/pages/quality/lib/format'
import {
	agentKey,
	exclusiveLongLabel,
	exclusiveShortLabel,
	profileName,
	spansSeveralProfiles,
	yieldAgentLabel,
} from '#/pages/quality/lib/sourcing'
import type { AgentYield, ProfileSummary } from '#/pages/quality/lib/types'

interface SourcingYieldCardProps {
	agents: readonly AgentYield[]
	profiles: readonly ProfileSummary[]
	expandedAgents: readonly string[]
	onToggleAgent: (key: string) => void
}

const COUNT_CLASS = 'text-right tabular-nums'

export function SourcingYieldCard({
	agents,
	profiles,
	expandedAgents,
	onToggleAgent,
}: SourcingYieldCardProps) {
	const showProfiles = spansSeveralProfiles(agents)
	return (
		<Card aria-label="Rendement des agents de recherche">
			<CardHeader>
				<CardTitle>Rendement des agents de recherche</CardTitle>
				<CardDescription>
					Leads trouvés par chaque agent de recherche et par mot-clé sur la
					période, avec leurs verdicts et les décisions humaines.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead className="w-10">
								<span className="sr-only">Mots-clés</span>
							</TableHead>
							<TableHead>Agent</TableHead>
							<TableHead className="text-right">Leads</TableHead>
							<TableHead className="text-right">Fiable</TableHead>
							<TableHead className="text-right">À vérifier</TableHead>
							<TableHead className="text-right">Écarté</TableHead>
							<TableHead className="text-right">Validés</TableHead>
							<TableHead className="text-right">Rejetés</TableHead>
							<TableHead className="text-right">Précision</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{agents.map((agent) => {
							const key = agentKey(agent)
							const expanded = expandedAgents.includes(key)
							const label = yieldAgentLabel(agent)
							const name = showProfiles
								? profileName(profiles, agent.profile_id)
								: null
							return (
								<Fragment key={key}>
									<TableRow data-agent={key}>
										<TableCell>
											<Button
												type="button"
												variant="ghost"
												size="icon-xs"
												aria-expanded={expanded}
												aria-label={`Mots-clés de ${label}`}
												onClick={() => onToggleAgent(key)}
											>
												{expanded ? <ChevronDownIcon /> : <ChevronRightIcon />}
											</Button>
										</TableCell>
										<TableCell>
											<div>{label}</div>
											{agent.agent_name === null ? (
												<div className="text-xs text-muted-foreground">
													Absent de la révision actuelle
												</div>
											) : null}
											{name === null ? null : (
												<div className="text-xs text-muted-foreground">
													{name}
												</div>
											)}
										</TableCell>
										<TableCell className={COUNT_CLASS}>
											{formatNumber(agent.leads)}
										</TableCell>
										<TableCell className={COUNT_CLASS}>
											{formatNumber(agent.verdicts.reliable)}
										</TableCell>
										<TableCell className={COUNT_CLASS}>
											{formatNumber(agent.verdicts.to_verify)}
										</TableCell>
										<TableCell className={COUNT_CLASS}>
											{formatNumber(agent.verdicts.discarded)}
										</TableCell>
										<TableCell className={COUNT_CLASS}>
											{formatNumber(agent.validated)}
										</TableCell>
										<TableCell className={COUNT_CLASS}>
											{formatNumber(agent.rejected)}
										</TableCell>
										<TableCell className={COUNT_CLASS}>
											{formatRatio(agent.precision)}
										</TableCell>
									</TableRow>
									{expanded && agent.keywords.length === 0 ? (
										<TableRow>
											<TableCell />
											<TableCell
												colSpan={8}
												className="text-xs text-muted-foreground"
											>
												Aucun mot-clé n'a trouvé de lead sur la période.
											</TableCell>
										</TableRow>
									) : null}
									{expanded
										? agent.keywords.map((keyword) => {
												const short = exclusiveShortLabel(keyword)
												return (
													<TableRow
														key={keyword.keyword}
														data-keyword={keyword.keyword}
														className="bg-muted/30"
													>
														<TableCell />
														<TableCell className="pl-6 text-sm">
															{keyword.keyword}
														</TableCell>
														<TableCell className={COUNT_CLASS}>
															<div>{formatNumber(keyword.leads)}</div>
															{short === null ? null : (
																<div
																	className="text-xs text-muted-foreground"
																	title={
																		exclusiveLongLabel(keyword) ?? undefined
																	}
																>
																	{short}
																</div>
															)}
														</TableCell>
														<TableCell className={COUNT_CLASS}>
															{formatNumber(keyword.verdicts.reliable)}
														</TableCell>
														<TableCell className={COUNT_CLASS}>
															{formatNumber(keyword.verdicts.to_verify)}
														</TableCell>
														<TableCell className={COUNT_CLASS}>
															{formatNumber(keyword.verdicts.discarded)}
														</TableCell>
														<TableCell className={COUNT_CLASS}>
															{formatNumber(keyword.validated)}
														</TableCell>
														<TableCell className={COUNT_CLASS}>
															{formatNumber(keyword.rejected)}
														</TableCell>
														<TableCell className={COUNT_CLASS}>
															{formatRatio(keyword.precision)}
														</TableCell>
													</TableRow>
												)
											})
										: null}
								</Fragment>
							)
						})}
					</TableBody>
				</Table>
			</CardContent>
		</Card>
	)
}
