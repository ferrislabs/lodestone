import { Badge } from '#/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { formatDateTime } from '#/pages/leads/lib/format'
import type {
	QualificationAnswer,
	QualificationResult,
} from '#/pages/leads/lib/types'
import {
	formatAgentScore,
	formatProbability,
	probabilityWidth,
} from '#/pages/leads/lib/verdict'

interface QualificationResultsProps {
	results: readonly QualificationResult[]
}

function providerText(result: QualificationResult): string {
	const name = result.provider_name
	if (name && result.model) return `${name} (${result.model})`
	return name ?? (result.model || 'Fournisseur inconnu')
}

function Measure({ answer }: { answer: QualificationAnswer }) {
	return (
		<li
			className="flex flex-col gap-1.5 rounded-md bg-muted/30 p-2.5"
			data-question={answer.question_id}
		>
			<div className="flex flex-wrap items-baseline gap-x-2">
				<p className="text-sm font-medium">{answer.wording}</p>
				<code className="font-mono text-[11px] text-muted-foreground">
					{answer.question_id}
				</code>
			</div>
			<p className="text-sm">
				<span className="text-muted-foreground">Réponse : </span>
				<span className="font-medium">{answer.choice_label}</span>
				<span className="text-muted-foreground">
					{' '}
					· confiance {formatProbability(answer.confidence)}
				</span>
			</p>
			<ul className="flex flex-col gap-1">
				{answer.probabilities.map((item) => (
					<li key={item.value} className="flex items-center gap-2 text-xs">
						<span className="w-28 shrink-0 truncate">{item.label}</span>
						<div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
							<div
								role="presentation"
								data-probability={item.value}
								className="h-full rounded-full bg-orange-500"
								style={{ width: `${probabilityWidth(item.probability)}%` }}
							/>
						</div>
						<span className="w-12 text-right font-mono tabular-nums">
							{formatProbability(item.probability)}
						</span>
					</li>
				))}
			</ul>
		</li>
	)
}

function AgentReport({ result }: { result: QualificationResult }) {
	const valid = result.status === 'valid'
	return (
		<article
			className="flex flex-col gap-3 rounded-lg border p-4"
			data-agent={result.agent_id}
			data-status={result.status}
		>
			<div className="flex flex-col gap-1">
				<div className="flex flex-wrap items-center gap-2">
					<h3 className="font-medium">{result.agent_name}</h3>
					<Badge variant={valid ? 'secondary' : 'destructive'}>
						{valid ? 'Valide' : 'Invalide'}
					</Badge>
					{valid ? (
						<span className="ml-auto text-sm">
							Score de l'agent :{' '}
							<span className="font-mono font-medium">
								{formatAgentScore(result.agent_score)}
							</span>
						</span>
					) : null}
				</div>
				<p className="text-xs text-muted-foreground">
					{providerText(result)} · {formatDateTime(result.created_at)}
				</p>
			</div>
			{result.agent_description ? (
				<p className="text-sm" data-part="evaluates">
					<span className="font-medium">Ce qu'il évalue : </span>
					{result.agent_description}
				</p>
			) : null}
			{result.summary ? (
				<p className="text-sm leading-relaxed" data-part="summary">
					{result.summary}
				</p>
			) : null}
			{valid && !result.summary ? (
				<p className="text-xs text-muted-foreground" data-part="no-summary">
					Pas de résumé : ce résultat précède le résumé par agent.
				</p>
			) : null}
			{result.error ? (
				<p className="text-sm text-destructive">{result.error}</p>
			) : null}
			{result.answers.length > 0 ? (
				<div className="flex flex-col gap-2" data-part="measures">
					<h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
						Mesures
					</h4>
					<ul className="flex flex-col gap-2">
						{result.answers.map((answer) => (
							<Measure key={answer.question_id} answer={answer} />
						))}
					</ul>
				</div>
			) : null}
		</article>
	)
}

export function QualificationResults({ results }: QualificationResultsProps) {
	return (
		<Card data-section="agents">
			<CardHeader>
				<CardTitle>Analyse par agent ({results.length})</CardTitle>
				<p className="text-sm text-muted-foreground">
					Chaque agent de qualification rapporte ce qu'il évalue, ce qu'il a
					observé et ses mesures. La synthèse ci-dessous les résume.
				</p>
			</CardHeader>
			<CardContent>
				{results.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucune qualification pour le moment.
					</p>
				) : (
					<div className="grid gap-4 lg:grid-cols-2">
						{results.map((result) => (
							<AgentReport key={result.agent_id} result={result} />
						))}
					</div>
				)}
			</CardContent>
		</Card>
	)
}
