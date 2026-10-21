import { TriangleAlert } from 'lucide-react'

import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { draftStatusLabel } from '#/pages/drafts/lib/labels'
import { formatDateTime } from '#/pages/leads/lib/format'
import { qualityWarningMessage } from '#/pages/leads/lib/quality'
import type { Assessment, SummaryState } from '#/pages/leads/lib/types'
import { ProvenanceLink } from './provenance-link'

interface LeadSummaryProps {
	assessment: Assessment | null
	summaryState: SummaryState | null
	onOpenDraft: (assessmentId: string) => void
	onRetry: () => void
	retrying: boolean
	retryError: string | null
}

function SummaryFailure({
	state,
	onRetry,
	retrying,
	retryError,
}: {
	state: SummaryState
	onRetry: () => void
	retrying: boolean
	retryError: string | null
}) {
	const reason = state.error ?? 'raison inconnue'
	return (
		<div
			data-summary-failed={state.failed ? 'true' : 'false'}
			className="flex flex-col gap-2 rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-800"
		>
			{state.failed ? (
				<p>
					<span className="font-medium">Bilan en échec : {reason}</span>
					<br />
					Après {state.attempts} échecs, une nouvelle tentative a lieu toutes
					les 6 heures.
				</p>
			) : (
				<p>
					Le bilan a échoué {state.attempts} fois ({reason}).
					{state.next_attempt_at
						? ` Nouvelle tentative : ${formatDateTime(state.next_attempt_at)}.`
						: ''}
				</p>
			)}
			<div>
				<Button
					variant="outline"
					size="sm"
					disabled={retrying}
					onClick={onRetry}
				>
					Réessayer
				</Button>
			</div>
			{retryError ? (
				<p role="alert" className="text-destructive">
					{retryError}
				</p>
			) : null}
		</div>
	)
}

export function LeadSummary({
	assessment,
	summaryState,
	onOpenDraft,
	onRetry,
	retrying,
	retryError,
}: LeadSummaryProps) {
	return (
		<Card data-section="summary">
			<CardHeader>
				<CardTitle>Synthèse</CardTitle>
				<p className="text-sm text-muted-foreground">
					Synthèse finale, rédigée par le modèle à partir des rapports de tous
					les agents ci-dessus.
				</p>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				{summaryState ? (
					<SummaryFailure
						state={summaryState}
						onRetry={onRetry}
						retrying={retrying}
						retryError={retryError}
					/>
				) : null}
				{assessment === null ? (
					<p className="text-sm text-muted-foreground">
						Pas encore de synthèse : elle est rédigée une fois que tous les
						agents de qualification ont répondu.
					</p>
				) : (
					<>
						<p>{assessment.summary}</p>
						{assessment.verdict_narrative ? (
							<p className="text-sm">
								<span className="font-medium">Verdict : </span>
								{assessment.verdict_narrative}
							</p>
						) : null}
						<p className="text-sm">
							<span className="font-medium">Angle d'approche : </span>
							{assessment.approach_angle}
						</p>
						{qualityWarningMessage(assessment.quality_warnings) ? (
							<div
								role="alert"
								data-quality-warning="true"
								className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800"
							>
								<TriangleAlert className="mt-0.5 size-4 shrink-0" />
								<span>
									{qualityWarningMessage(assessment.quality_warnings)}
								</span>
							</div>
						) : null}
						{assessment.evidence_verified ? null : (
							<output className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
								<TriangleAlert className="mt-0.5 size-4 shrink-0" />
								<span>
									Preuves non vérifiées : aucune citation n'a pu être retrouvée
									mot pour mot dans les signaux.
								</span>
							</output>
						)}
						{assessment.evidence.length > 0 ? (
							<div className="flex flex-col gap-3">
								<span className="text-sm font-medium">
									Preuves ({assessment.evidence.length})
								</span>
								{assessment.evidence.map((item) => (
									<blockquote
										key={`${item.provenance}-${item.quote}`}
										className="flex flex-col gap-1 border-l-2 border-orange-300 pl-3"
									>
										<p className="text-sm italic">{item.quote}</p>
										<ProvenanceLink provenance={item.provenance} />
									</blockquote>
								))}
							</div>
						) : null}
						<div className="flex flex-wrap gap-x-4 text-xs text-muted-foreground">
							<span>Modèle : {assessment.model}</span>
							<span>Prompt : {assessment.prompt_version}</span>
							<span>Révision du profil : {assessment.profile_revision}</span>
							<span>{formatDateTime(assessment.created_at)}</span>
						</div>
						{assessment.draft ? (
							<div className="flex flex-col gap-2 rounded-lg border bg-muted/30 p-3">
								<div className="flex items-center gap-2">
									<span className="text-sm font-medium">Brouillon</span>
									<Badge variant="secondary">
										{draftStatusLabel(assessment.draft.status)}
									</Badge>
									<Button
										className="ml-auto"
										variant="outline"
										size="sm"
										onClick={() => onOpenDraft(assessment.id)}
									>
										{assessment.draft.status === 'pending'
											? 'Ouvrir dans la file de validation'
											: 'Ouvrir le brouillon'}
									</Button>
								</div>
								<p className="text-sm font-medium">
									{assessment.draft.subject}
								</p>
							</div>
						) : null}
					</>
				)}
			</CardContent>
		</Card>
	)
}
