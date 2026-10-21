import { Badge } from '#/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { escalationLabel } from '#/pages/leads/lib/agreement'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { AgentFailure, QualificationResult } from '#/pages/leads/lib/types'
import { formatAgentScore } from '#/pages/leads/lib/verdict'
import { AgentFailureNotice } from '#/pages/leads/ui/agent-failure-notice'
import { AnswerMeasure } from '#/pages/leads/ui/answer-measure'

interface QualificationResultsProps {
	results: readonly QualificationResult[]
	failure: AgentFailure | null
	onRetry: () => void
	retrying: boolean
	retryError: string | null
}

function providerText(result: QualificationResult): string {
	const name = result.provider_name
	if (name && result.model) return `${name} (${result.model})`
	return name ?? (result.model || 'Fournisseur inconnu')
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
					{result.escalated ? (
						<Badge variant="outline" data-part="escalated">
							{escalationLabel(result.provider_name, result.model)}
						</Badge>
					) : null}
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

export function QualificationResults({
	results,
	failure,
	onRetry,
	retrying,
	retryError,
}: QualificationResultsProps) {
	return (
		<Card data-section="agents">
			<CardHeader>
				<CardTitle>Analyse par agent ({results.length})</CardTitle>
				<p className="text-sm text-muted-foreground">
					Chaque agent de qualification rapporte ce qu'il évalue, ce qu'il a
					observé et ses mesures. La synthèse ci-dessous les résume.
				</p>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				{failure ? (
					<AgentFailureNotice
						failure={failure}
						onRetry={onRetry}
						retrying={retrying}
						retryError={retryError}
					/>
				) : null}
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
