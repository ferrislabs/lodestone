import { ArrowLeft } from 'lucide-react'
import type { ReactNode } from 'react'

import { Button } from '#/components/ui/button'
import { Skeleton } from '#/components/ui/skeleton'
import { isEdited } from '#/pages/drafts/lib/edit'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftContext } from './draft-context'
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
}: DraftReviewProps) {
	const pending = entry?.draft.status === 'pending'
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<div>
				<Button variant="ghost" size="sm" onClick={onBack}>
					<ArrowLeft />
					Back to drafts
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
					Could not load this draft: {errorMessage}
				</p>
			) : null}
			{state === 'ready' && entry ? (
				<>
					<DraftContext entry={entry} onOpenLead={onOpenLead} />
					{isEdited(entry.draft) ? <OriginalDraft draft={entry.draft} /> : null}
					{pending ? (
						<>
							<section className="flex flex-col gap-3">
								<h2 className="text-lg font-semibold">Draft</h2>
								{editor}
							</section>
							<section className="flex flex-col gap-3">
								<h2 className="text-lg font-semibold">Decision</h2>
								{decisionError ? (
									<p role="alert" className="text-sm text-destructive">
										{decisionError}
									</p>
								) : null}
								{dirty ? (
									<p className="text-sm text-muted-foreground">
										Save your changes before deciding.
									</p>
								) : null}
								<div className="flex flex-wrap items-center gap-3">
									<Button disabled={dirty || deciding} onClick={onApprove}>
										Approve (marks ready, nothing is sent)
									</Button>
									<Button
										variant="outline"
										disabled={deciding || rejectOpen}
										onClick={onStartReject}
									>
										Reject
									</Button>
								</div>
								{rejectOpen ? rejectForm : null}
							</section>
						</>
					) : (
						<p className="text-sm text-muted-foreground">
							This draft was already {entry.draft.status}.
						</p>
					)}
				</>
			) : null}
		</div>
	)
}
