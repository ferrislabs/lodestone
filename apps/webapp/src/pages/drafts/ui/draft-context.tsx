import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { formatDateTime } from '#/pages/leads/lib/format'
import { ProvenanceLink } from '#/pages/leads/ui/provenance-link'
import { ScoreBar } from '#/pages/leads/ui/score-bar'

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
				<ScoreBar
					score={entry.lead.score}
					max={Math.max(entry.lead.score, 1)}
				/>
				<p>{entry.summary}</p>
				<p className="text-sm">
					<span className="font-medium">Approach angle: </span>
					{entry.approach_angle}
				</p>
				<p className="text-sm">
					<span className="font-medium">Best contact: </span>
					{entry.contact
						? `${entry.contact.address} (${entry.contact.method}, confidence ${Math.round(entry.contact.confidence * 100)}%)`
						: 'No contact found'}
				</p>
				<div className="flex flex-col gap-3">
					<span className="text-sm font-medium">
						Evidence ({entry.evidence.length})
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
