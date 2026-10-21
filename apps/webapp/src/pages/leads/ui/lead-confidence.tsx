import {
	evidenceComponentRows,
	evidenceSignalRows,
	verdictReasonLabel,
} from '#/pages/leads/lib/confidence'
import type { Lead } from '#/pages/leads/lib/types'
import { formatConfidence } from '#/pages/leads/lib/verdict'

interface LeadConfidenceProps {
	lead: Lead
}

export function LeadConfidence({ lead }: LeadConfidenceProps) {
	const evidence = lead.evidence ?? null
	const reason = lead.verdict_reason ?? null
	return (
		<>
			<div>
				<dt className="text-muted-foreground">Confiance</dt>
				<dd className="font-mono">{formatConfidence(lead.confidence)}</dd>
			</div>
			<div>
				<dt className="text-muted-foreground">Avis des agents</dt>
				<dd className="font-mono">{formatConfidence(lead.opinion)}</dd>
			</div>
			<div>
				<dt className="text-muted-foreground">Solidité des preuves</dt>
				<dd className="font-mono">
					{formatConfidence(evidence ? evidence.strength : null)}
				</dd>
			</div>
			{evidence || reason ? (
				<details
					data-section="confidence-calculation"
					open={reason !== null}
					className="text-sm sm:col-span-2 lg:col-span-4"
				>
					<summary className="cursor-pointer text-muted-foreground">
						Calcul
					</summary>
					<div className="mt-1 flex flex-col gap-1">
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
