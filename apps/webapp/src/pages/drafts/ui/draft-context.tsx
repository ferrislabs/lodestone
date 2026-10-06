import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { contactMethodLabel } from '#/lib/labels'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { formatDateTime, formatRatio } from '#/pages/leads/lib/format'
import { ConfidenceBar } from '#/pages/leads/ui/confidence-bar'
import { ProvenanceLink } from '#/pages/leads/ui/provenance-link'
import { ScoreBar } from '#/pages/leads/ui/score-bar'
import { VerdictBadge } from '#/pages/leads/ui/verdict-badge'

interface DraftContextProps {
	entry: DraftEntry
	onOpenLead: (leadId: string) => void
}

export function DraftContext({ entry, onOpenLead }: DraftContextProps) {
	return (
		<Card>
			<CardHeader>
				<CardTitle className="flex flex-wrap items-baseline gap-x-3">
					<button
						type="button"
						className="hover:underline"
						onClick={() => onOpenLead(entry.lead.id)}
					>
						{entry.lead.domain}
					</button>
					<span className="text-sm font-normal text-muted-foreground">
						{entry.lead.profile.name} - {formatDateTime(entry.created_at)}
					</span>
				</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				<div className="flex flex-wrap items-center gap-4">
					<VerdictBadge verdict={entry.lead.verdict} />
					<ConfidenceBar confidence={entry.lead.confidence} />
					<ScoreBar
						score={entry.lead.score}
						max={Math.max(entry.lead.score, 1)}
					/>
				</div>
				<p>{entry.summary}</p>
				{entry.verdict_narrative ? (
					<p className="text-sm">
						<span className="font-medium">Verdict : </span>
						{entry.verdict_narrative}
					</p>
				) : null}
				<p className="text-sm">
					<span className="font-medium">Angle d'approche : </span>
					{entry.approach_angle}
				</p>
				<p className="text-sm">
					<span className="font-medium">Meilleur contact : </span>
					{entry.contact
						? `${entry.contact.address} (${contactMethodLabel(entry.contact.method)}, confiance ${formatRatio(entry.contact.confidence)})`
						: 'Aucun contact trouvé'}
				</p>
				{entry.evidence_verified ? null : (
					<output className="block rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
						Preuves non vérifiées : au moins une citation n'a pas pu être
						retrouvée mot pour mot dans les signaux.
					</output>
				)}
				<div className="flex flex-col gap-3">
					<span className="text-sm font-medium">
						Preuves ({entry.evidence.length})
					</span>
					{entry.evidence.map((item) => (
						<blockquote
							key={`${item.provenance}-${item.quote}`}
							className="flex flex-col gap-1 border-l-2 border-orange-300 pl-3"
						>
							<p className="text-sm italic">{item.quote}</p>
							<ProvenanceLink provenance={item.provenance} />
						</blockquote>
					))}
				</div>
			</CardContent>
		</Card>
	)
}
