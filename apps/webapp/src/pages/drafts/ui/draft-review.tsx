import { ArrowLeft } from 'lucide-react'
import type { ReactNode } from 'react'

import { Button } from '#/components/ui/button'
import { Skeleton } from '#/components/ui/skeleton'
import { isEdited } from '#/pages/drafts/lib/edit'
import { draftStatusLabel } from '#/pages/drafts/lib/labels'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftContext } from './draft-context'
import { DraftSuggestion } from './draft-suggestion'
import { OriginalDraft } from './original-draft'

interface DraftReviewProps {
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	entry: DraftEntry | null
	editor: ReactNode
	rejectForm: ReactNode
	rejectOpen: boolean
	dirty: boolean
	deciding: boolean
	decisionError: string | null
	onBack: () => void
	onOpenLead: (leadId: string) => void
	onApprove: () => void
	onStartReject: () => void
	onCopy: (text: string) => void
}

export function DraftReview({
	state,
	errorMessage,
	entry,
	editor,
	rejectForm,
	rejectOpen,
	dirty,
	deciding,
	decisionError,
	onBack,
	onOpenLead,
	onApprove,
	onStartReject,
	onCopy,
}: DraftReviewProps) {
	const pending = entry?.draft.status === 'pending'
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<div>
				<Button variant="ghost" size="sm" onClick={onBack}>
					<ArrowLeft />
					Retour aux brouillons
				</Button>
			</div>
			{state === 'loading' ? (
				<div className="flex flex-col gap-3" aria-busy="true">
					<Skeleton className="h-8 w-64" />
					<Skeleton className="h-48 w-full" />
					<Skeleton className="h-64 w-full" />
				</div>
			) : null}
			{state === 'error' ? (
				<p role="alert" className="text-destructive">
					Impossible de charger ce brouillon : {errorMessage}
				</p>
			) : null}
			{state === 'ready' && entry ? (
				<>
					<DraftContext entry={entry} onOpenLead={onOpenLead} />
					{isEdited(entry.draft) ? <OriginalDraft draft={entry.draft} /> : null}
					{pending ? (
						<>
							<DraftSuggestion draft={entry.draft} onCopy={onCopy} />
							<section className="flex flex-col gap-3">
								<h2 className="text-lg font-semibold">Brouillon</h2>
								{editor}
							</section>
							<section className="flex flex-col gap-3">
								<h2 className="text-lg font-semibold">Décision</h2>
								{decisionError ? (
									<p role="alert" className="text-sm text-destructive">
										{decisionError}
									</p>
								) : null}
								{dirty ? (
									<p className="text-sm text-muted-foreground">
										Enregistrez vos modifications avant de décider.
									</p>
								) : null}
								<div className="flex flex-wrap items-center gap-3">
									<Button disabled={dirty || deciding} onClick={onApprove}>
										Approuver (marque comme prêt, rien n'est envoyé)
									</Button>
									<Button
										variant="outline"
										disabled={deciding || rejectOpen}
										onClick={onStartReject}
									>
										Rejeter
									</Button>
								</div>
								{rejectOpen ? rejectForm : null}
							</section>
						</>
					) : (
						<p className="text-sm text-muted-foreground">
							Ce brouillon a déjà été{' '}
							{draftStatusLabel(entry.draft.status).toLowerCase()}.
						</p>
					)}
				</>
			) : null}
		</div>
	)
}
