import {
	evidenceComponentRows,
	evidenceSignalRows,
	indicatorCapLabel,
	verdictReasonLabel,
} from '#/pages/leads/lib/confidence'
import { formatLlmScore } from '#/pages/leads/lib/scoring'
import type { Lead } from '#/pages/leads/lib/types'
import { formatConfidence } from '#/pages/leads/lib/verdict'

interface LeadConfidenceProps {
	lead: Lead
}

export function LeadConfidence({ lead }: LeadConfidenceProps) {
	const evidence = lead.evidence ?? null
	const reason = lead.verdict_reason ?? null
	const cap = lead.indicator_cap ?? null
	const hasIndicators = evidence !== null || lead.indicator_confidence != null
	return (
		<>
			<div>
				<dt className="text-muted-foreground">Score</dt>
				<dd className="font-mono" data-part="llm-score">
					{formatLlmScore(lead.llm_score)}
				</dd>
			</div>
			<div>
				<dt className="text-muted-foreground">Confiance du verdict</dt>
				<dd className="font-mono" data-part="verdict-confidence">
					{formatConfidence(lead.confidence)}
				</dd>
			</div>
			{hasIndicators || reason ? (
				<details
					data-section="confidence-calculation"
					open={reason !== null}
					className="text-sm sm:col-span-2 lg:col-span-4"
				>
					<summary className="cursor-pointer text-muted-foreground">
						Calcul
					</summary>
					<div className="mt-1 flex flex-col gap-2">
						{hasIndicators ? (
							<section
								aria-label="Indicateurs"
								data-part="indicators"
								className="flex flex-col gap-1"
							>
								<h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
									Indicateurs
								</h4>
								<p className="text-xs text-muted-foreground">
									Calculés par le système (avis des agents × preuves) et donnés
									aux agents de scoring : ils ne décident pas du verdict.
								</p>
								<ul className="grid gap-x-8 gap-y-1 sm:grid-cols-2 lg:grid-cols-4">
									<li
										data-indicator="opinion"
										className="flex justify-between gap-2"
									>
										<span className="text-muted-foreground">
											Avis des agents
										</span>
										<span className="font-mono">
											{formatConfidence(lead.opinion)}
										</span>
									</li>
									<li
										data-indicator="evidence"
										className="flex justify-between gap-2"
									>
										<span className="text-muted-foreground">
											Solidité des preuves
										</span>
										<span className="font-mono">
											{formatConfidence(evidence ? evidence.strength : null)}
										</span>
									</li>
									<li
										data-indicator="confidence"
										className="flex justify-between gap-2"
									>
										<span className="text-muted-foreground">
											Confiance des indicateurs
										</span>
										<span className="font-mono">
											{formatConfidence(lead.indicator_confidence)}
										</span>
									</li>
								</ul>
							</section>
						) : null}
						{evidence ? (
							<ul className="grid gap-x-8 gap-y-1 sm:grid-cols-2 lg:grid-cols-4">
								{evidenceComponentRows(evidence).map((row) => (
									<li
										key={row.key}
										data-evidence-component={row.key}
										className="flex justify-between gap-2"
									>
										<span className="text-muted-foreground">{row.label}</span>
										<span className="font-mono">
											{formatConfidence(row.value)}
										</span>
									</li>
								))}
							</ul>
						) : null}
						{evidence ? (
							<ul className="grid gap-x-8 gap-y-1 sm:grid-cols-2 lg:grid-cols-4">
								{evidenceSignalRows(evidence).map((row) => (
									<li
										key={row.key}
										data-evidence-signals={row.key}
										className="flex justify-between gap-2"
									>
										<span className="text-muted-foreground">{row.label}</span>
										<span className="font-mono">{row.value}</span>
									</li>
								))}
							</ul>
						) : null}
						{cap && cap !== reason ? (
							<p data-indicator-cap={cap} className="text-amber-700">
								{indicatorCapLabel(cap)}
							</p>
						) : null}
						{reason ? (
							<p data-verdict-reason={reason} className="text-amber-700">
								{verdictReasonLabel(reason)}
							</p>
						) : null}
					</div>
				</details>
			) : null}
		</>
	)
}
