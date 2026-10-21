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

function AnswerBlock({ answer }: { answer: QualificationAnswer }) {
	return (
		<div className="flex flex-col gap-2" data-question={answer.question_id}>
			<p className="text-sm font-medium">{answer.wording}</p>
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
						<span className="w-32 shrink-0 truncate">{item.label}</span>
						<div className="h-2 flex-1 overflow-hidden rounded-full bg-muted">
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
		</div>
	)
}

function ResultCard({ result }: { result: QualificationResult }) {
	const valid = result.status === 'valid'
	return (
		<article
			className="flex flex-col gap-3 rounded-lg border p-4"
			data-agent={result.agent_id}
			data-status={result.status}
		>
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
			{result.error ? (
				<p className="text-sm text-destructive">{result.error}</p>
			) : null}
			{result.answers.map((answer) => (
				<AnswerBlock key={answer.question_id} answer={answer} />
			))}
		</article>
	)
}

export function QualificationResults({ results }: QualificationResultsProps) {
	return (
		<Card>
			<CardHeader>
				<CardTitle>Qualification ({results.length})</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				{results.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucune qualification pour le moment.
					</p>
				) : null}
				{results.map((result) => (
					<ResultCard key={result.agent_id} result={result} />
				))}
			</CardContent>
		</Card>
	)
}
