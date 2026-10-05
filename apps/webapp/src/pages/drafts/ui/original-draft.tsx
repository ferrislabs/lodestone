import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import type { DraftEntry } from '#/pages/drafts/lib/types'

interface OriginalDraftProps {
	draft: DraftEntry['draft']
}

export function OriginalDraft({ draft }: OriginalDraftProps) {
	return (
		<Card>
			<CardHeader>
				<CardTitle>Texte original du modèle</CardTitle>
			</CardHeader>
			<CardContent className="grid gap-4 md:grid-cols-2">
				<div className="flex flex-col gap-2">
					<span className="text-xs font-medium uppercase text-muted-foreground">
						Original
					</span>
					<p className="text-sm font-medium">
						{draft.original_subject ?? draft.subject}
					</p>
					<p className="whitespace-pre-wrap text-sm">
						{draft.original_body ?? draft.body}
					</p>
				</div>
				<div className="flex flex-col gap-2">
					<span className="text-xs font-medium uppercase text-muted-foreground">
						Actuel
					</span>
					<p className="text-sm font-medium">{draft.subject}</p>
					<p className="whitespace-pre-wrap text-sm">{draft.body}</p>
				</div>
			</CardContent>
		</Card>
	)
}
