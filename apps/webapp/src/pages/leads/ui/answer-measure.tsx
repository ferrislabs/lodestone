import { Badge } from '#/components/ui/badge'
import { agreementLabel } from '#/pages/leads/lib/agreement'
import type { QualificationAnswer } from '#/pages/leads/lib/types'
import { formatProbability, probabilityWidth } from '#/pages/leads/lib/verdict'
import { ProvenanceLink } from '#/pages/leads/ui/provenance-link'

function EvidenceSource({ source }: { source: string }) {
	if (source === 'lead') {
		return <span>Faits calculés par le système</span>
	}
	return <ProvenanceLink provenance={source} />
}

function Grounding({ answer }: { answer: QualificationAnswer }) {
	if (!answer.grounded) {
		return (
			<p
				className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs"
				data-part="ungrounded"
			>
				<Badge variant="outline">Non étayé — ramené à « Inconnu »</Badge>
				{answer.original_choice_label ? (
					<span className="text-muted-foreground">
						Choix initial du modèle : {answer.original_choice_label}
					</span>
				) : null}
			</p>
		)
	}
	if (!answer.evidence) return null
	return (
		<div
			className="flex flex-col gap-0.5 border-l-2 pl-2 text-xs"
			data-part="evidence"
		>
			<p className="italic">« {answer.evidence} »</p>
			{answer.evidence_source ? (
				<p className="text-muted-foreground" data-part="evidence-source">
					Source : <EvidenceSource source={answer.evidence_source} />
				</p>
			) : null}
		</div>
	)
}

export function AnswerMeasure({
	answer,
	samples,
}: {
	answer: QualificationAnswer
	samples: number
}) {
	const agreement = agreementLabel(answer.agreement, samples)
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
			{agreement ? (
				<p className="text-xs text-muted-foreground" data-part="agreement">
					{agreement}
				</p>
			) : null}
			<Grounding answer={answer} />
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
