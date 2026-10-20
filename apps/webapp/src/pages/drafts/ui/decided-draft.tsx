import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftActions } from './draft-actions'

interface DecidedDraftProps {
	draft: DraftEntry['draft']
	contactAddress: string | null
	onCopy: (text: string) => void
}

export function DecidedDraft({
	draft,
	contactAddress,
	onCopy,
}: DecidedDraftProps) {
	return (
		<Card data-section="decided-draft">
			<CardHeader>
				<CardTitle>Brouillon</CardTitle>
			</CardHeader>
			<CardContent className="flex flex-col gap-3">
				<p className="font-medium">{draft.subject}</p>
				<p className="whitespace-pre-wrap text-sm">{draft.body}</p>
				<DraftActions
					subject={draft.subject}
					body={draft.body}
					contactAddress={contactAddress}
					onCopy={onCopy}
				/>
			</CardContent>
		</Card>
	)
}
