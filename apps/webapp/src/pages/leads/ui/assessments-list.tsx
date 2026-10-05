import { Badge } from '#/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { draftStatusLabel } from '#/pages/drafts/lib/labels'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { Assessment } from '#/pages/leads/lib/types'
import { ProvenanceLink } from './provenance-link'

interface AssessmentsListProps {
	assessments: readonly Assessment[]
}

export function AssessmentsList({ assessments }: AssessmentsListProps) {
	return (
		<Card>
			<CardHeader>
				<CardTitle>Évaluations ({assessments.length})</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-col gap-6">
				{assessments.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucune évaluation pour le moment.
					</p>
				) : null}
				{assessments.map((assessment) => (
					<article key={assessment.id} className="flex flex-col gap-3">
						<div className="flex flex-wrap gap-x-4 text-xs text-muted-foreground">
							<span>Modèle : {assessment.model}</span>
							<span>Prompt : {assessment.prompt_version}</span>
							<span>Révision du profil : {assessment.profile_revision}</span>
							<span>{formatDateTime(assessment.created_at)}</span>
						</div>
						<p>{assessment.summary}</p>
						<p className="text-sm">
							<span className="font-medium">Angle d'approche : </span>
							{assessment.approach_angle}
						</p>
						{assessment.evidence.map((item) => (
							<blockquote
								key={`${item.provenance}-${item.quote}`}
								className="flex flex-col gap-1 border-l-2 border-orange-300 pl-3"
							>
								<p className="text-sm italic">{item.quote}</p>
								<ProvenanceLink provenance={item.provenance} />
							</blockquote>
						))}
						{assessment.draft ? (
							<div className="flex flex-col gap-2 rounded-lg border bg-muted/30 p-3">
								<div className="flex items-center gap-2">
									<span className="text-sm font-medium">Brouillon</span>
									<Badge variant="secondary">
										{draftStatusLabel(assessment.draft.status)}
									</Badge>
								</div>
								<p className="text-sm font-medium">
									{assessment.draft.subject}
								</p>
								<p className="whitespace-pre-wrap text-sm">
									{assessment.draft.body}
								</p>
							</div>
						) : null}
					</article>
				))}
			</CardContent>
		</Card>
	)
}
