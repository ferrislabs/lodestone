import { Button } from '#/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { formatDateTime } from '#/pages/leads/lib/format'

interface DraftSuggestionProps {
	draft: DraftEntry['draft']
	onCopy: (text: string) => void
}

export function DraftSuggestion({ draft, onCopy }: DraftSuggestionProps) {
	const subject = draft.suggested_subject
	const body = draft.suggested_body
	if (subject == null || body == null) return null
	return (
		<Card data-section="suggestion">
			<CardHeader>
				<CardTitle>Nouvelle suggestion du modèle</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-col gap-3">
				{draft.outdated_at ? (
					<output className="block rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">
						Le bilan a été mis à jour après votre modification (
						{formatDateTime(draft.outdated_at)}). Votre brouillon est conservé ;
						la suggestion ci-dessous est en lecture seule.
					</output>
				) : null}
				<div className="flex flex-col gap-2">
					<div className="flex items-center justify-between gap-2">
						<span className="text-xs font-medium uppercase text-muted-foreground">
							Objet suggéré
						</span>
						<Button variant="outline" size="sm" onClick={() => onCopy(subject)}>
							Copier l'objet
						</Button>
					</div>
					<p className="text-sm font-medium">{subject}</p>
				</div>
				<div className="flex flex-col gap-2">
					<div className="flex items-center justify-between gap-2">
						<span className="text-xs font-medium uppercase text-muted-foreground">
							Corps suggéré
						</span>
						<Button variant="outline" size="sm" onClick={() => onCopy(body)}>
							Copier le corps
						</Button>
					</div>
					<p className="whitespace-pre-wrap text-sm">{body}</p>
				</div>
			</CardContent>
		</Card>
	)
}
