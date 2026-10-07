import { Badge } from '#/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { cn } from '#/lib/utils'
import { verdictReasonLabel } from '#/pages/leads/lib/confidence'
import { formatDateTime } from '#/pages/leads/lib/format'
import {
	calibrationLabel,
	declaredChoiceNote,
	formatLlmScore,
	formatPercentValue,
	scoringProviderText,
	scoringWeightLabel,
	type VerdictProbabilityRow,
	verdictProbabilityRows,
} from '#/pages/leads/lib/scoring'
import type { Lead, ScoringResult, Verdict } from '#/pages/leads/lib/types'
import { formatConfidence } from '#/pages/leads/lib/verdict'
import { AnswerMeasure } from '#/pages/leads/ui/answer-measure'
import { VerdictBadge } from '#/pages/leads/ui/verdict-badge'

interface ScoringResultsProps {
	results: readonly ScoringResult[]
	lead: Lead
}

const BAR_STYLES: Record<Verdict, string> = {
	fiable: 'bg-emerald-500',
	a_verifier: 'bg-amber-500',
	ecarte: 'bg-red-500',
}

function VerdictBars({
	rows,
	part = 'verdict-probabilities',
}: {
	rows: readonly VerdictProbabilityRow[]
	part?: string
}) {
	return (
		<ul className="flex flex-col gap-1" data-part={part}>
			{rows.map((row) => (
				<li key={row.key} className="flex items-center gap-2 text-xs">
					<span className="w-20 shrink-0">{row.label}</span>
					<div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
						<div
							role="presentation"
							data-verdict-probability={row.key}
							className={cn('h-full rounded-full', BAR_STYLES[row.key])}
							style={{ width: `${Math.round(row.percent)}%` }}
						/>
					</div>
					<span className="w-14 text-right font-mono tabular-nums">
						{formatPercentValue(row.percent)}
					</span>
				</li>
			))}
		</ul>
	)
}

function AgentScoring({ result }: { result: ScoringResult }) {
	const valid = result.status === 'valid'
	const verdict = result.verdict ?? null
	const note = verdict ? declaredChoiceNote(verdict) : null
	const calibration = result.calibration ?? null
	return (
		<article
			className="flex flex-col gap-3 rounded-lg border p-4"
			data-scoring-agent={result.agent_id}
			data-status={result.status}
		>
			<div className="flex flex-col gap-1">
				<div className="flex flex-wrap items-center gap-2">
					<h3 className="font-medium">{result.agent_name}</h3>
					<Badge variant={valid ? 'secondary' : 'destructive'}>
						{valid ? 'Valide' : 'Invalide'}
					</Badge>
					{result.weight != null ? (
						<span className="text-xs text-muted-foreground">
							{scoringWeightLabel(result.weight)}
						</span>
					) : null}
					{valid ? (
						<span className="ml-auto text-sm">
							Score :{' '}
							<span className="font-mono font-medium" data-part="agent-score">
								{formatLlmScore(result.score)}
							</span>
						</span>
					) : null}
				</div>
				<p className="text-xs text-muted-foreground" data-part="model">
					{scoringProviderText(result)} · {formatDateTime(result.created_at)}
				</p>
			</div>
			{result.agent_description ? (
				<p className="text-sm" data-part="evaluates">
					<span className="font-medium">Ce qu'il évalue : </span>
					{result.agent_description}
				</p>
			) : null}
			{result.error ? (
				<p className="text-sm text-destructive">{result.error}</p>
			) : null}
			{verdict ? (
				<div className="flex flex-col gap-2" data-part="verdict">
					<div className="flex flex-wrap items-center gap-2 text-sm">
						<VerdictBadge verdict={verdict.choice} />
						<span className="text-muted-foreground">
							probabilité {formatConfidence(verdict.confidence)}
						</span>
					</div>
					{calibration ? (
						<div className="flex flex-col gap-2" data-part="calibration">
							<Badge variant="outline" data-part="calibration-badge">
								{calibrationLabel(calibration.n)}
							</Badge>
							<p className="text-xs font-medium text-muted-foreground">
								Probabilités brutes de l'agent
							</p>
							<VerdictBars
								rows={verdictProbabilityRows(verdict.probabilities)}
								part="verdict-probabilities-raw"
							/>
							<p className="text-xs font-medium text-muted-foreground">
								Probabilités calibrées, utilisées dans le résultat combiné
							</p>
							<VerdictBars
								rows={verdictProbabilityRows(calibration.probabilities)}
								part="verdict-probabilities-calibrated"
							/>
						</div>
					) : (
						<VerdictBars rows={verdictProbabilityRows(verdict.probabilities)} />
					)}
					{note ? (
						<p className="text-xs text-muted-foreground" data-part="declared">
							{note}
						</p>
					) : null}
				</div>
			) : null}
			{result.reasons.length > 0 ? (
				<div className="flex flex-col gap-1" data-part="reasons">
					<h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
						Raisons
					</h4>
					<ul className="flex list-disc flex-col gap-0.5 pl-4 text-sm">
						{result.reasons.map((reason) => (
							<li key={reason}>{reason}</li>
						))}
					</ul>
				</div>
			) : null}
			{result.criteria.length > 0 ? (
				<div className="flex flex-col gap-2" data-part="criteria">
					<h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
						Critères
					</h4>
					<ul className="flex flex-col gap-2">
						{result.criteria.map((answer) => (
							<AnswerMeasure
								key={answer.question_id}
								answer={answer}
								samples={result.provenance.samples}
							/>
						))}
					</ul>
				</div>
			) : null}
		</article>
	)
}

function Combined({ lead }: { lead: Lead }) {
	const rows = verdictProbabilityRows(lead.verdict_probabilities)
	const reason = lead.verdict_reason ?? null
	return (
		<section
			aria-label="Résultat combiné"
			data-part="combined"
			className="flex flex-col gap-3 rounded-lg border-2 border-orange-200 bg-orange-50/40 p-4"
		>
			<h3 className="font-medium">Résultat combiné</h3>
			<dl className="grid gap-x-8 gap-y-1 text-sm sm:grid-cols-3">
				<div>
					<dt className="text-muted-foreground">Score</dt>
					<dd className="font-mono" data-part="combined-score">
						{formatLlmScore(lead.llm_score)}
					</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Verdict</dt>
					<dd>
						<VerdictBadge verdict={lead.verdict} />
					</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Confiance du verdict</dt>
					<dd className="font-mono" data-part="combined-confidence">
						{formatConfidence(lead.confidence)}
					</dd>
				</div>
			</dl>
			{rows.length > 0 ? <VerdictBars rows={rows} /> : null}
			{reason ? (
				<p data-verdict-reason={reason} className="text-sm text-amber-700">
					{verdictReasonLabel(reason)}
				</p>
			) : null}
		</section>
	)
}

export function ScoringResults({ results, lead }: ScoringResultsProps) {
	return (
		<Card data-section="scoring">
			<CardHeader>
				<CardTitle>Scoring ({results.length})</CardTitle>
				<p className="text-sm text-muted-foreground">
					Chaque agent de scoring décide d'un score et d'un verdict à partir des
					rapports de qualification et des indicateurs. Le résultat combiné est
					la moyenne pondérée par le poids des agents.
				</p>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				{results.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucun agent de scoring n'a répondu pour le moment.
					</p>
				) : (
					<div className="grid gap-4 lg:grid-cols-2">
						{results.map((result) => (
							<AgentScoring key={result.agent_id} result={result} />
						))}
					</div>
				)}
				<Combined lead={lead} />
			</CardContent>
		</Card>
	)
}
