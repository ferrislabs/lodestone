import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from '#/components/ui/card'
import { formatDateTime } from '#/lib/format'
import { sourceLabel } from '#/lib/labels'
import { externalHref } from '#/pages/leads/lib/format'
import {
	type DroppedEntry,
	dropReasonLabel,
	dropReviewLabel,
} from '#/pages/quality/lib/dropped'
import type { DroppedCandidate, DropReview } from '#/pages/quality/lib/types'

interface WordsProps {
	label: string
	words: readonly string[]
	variant: 'secondary' | 'outline'
}

function Words({ label, words, variant }: WordsProps) {
	if (words.length === 0) return null
	return (
		<section aria-label={label} className="flex flex-col gap-1">
			<h4 className="text-xs font-medium">{label}</h4>
			<ul className="flex flex-wrap gap-1">
				{words.map((word) => (
					<li key={word}>
						<Badge variant={variant}>{word}</Badge>
					</li>
				))}
			</ul>
		</section>
	)
}

interface DroppedCardProps {
	entry: DroppedEntry
	onReview: (candidate: DroppedCandidate, review: DropReview) => void
	onOpenLead: (leadId: string) => void
}

export function DroppedCard({ entry, onReview, onOpenLead }: DroppedCardProps) {
	const { candidate, leadId, error, pending } = entry
	const href = externalHref(candidate.provenance)
	const context = [
		candidate.domain,
		sourceLabel(candidate.source),
		candidate.agent,
	].filter((part) => part)
	return (
		<Card data-candidate={candidate.id}>
			<CardHeader>
				<div className="flex flex-wrap items-center gap-2">
					<CardTitle>{candidate.title}</CardTitle>
					<Badge variant="outline">{dropReasonLabel(candidate.reason)}</Badge>
				</div>
				<CardDescription>{context.join(' · ')}</CardDescription>
				<p className="text-xs text-muted-foreground">
					Observée le {formatDateTime(candidate.observed_at)} · écartée le{' '}
					{formatDateTime(candidate.dropped_at)}
				</p>
			</CardHeader>
			<CardContent className="flex flex-col gap-3">
				<Words
					label="Mots-clés trouvés"
					words={candidate.matched}
					variant="secondary"
				/>
				<Words
					label="Mots exclus"
					words={candidate.excluded}
					variant="outline"
				/>
				{candidate.excerpts.map((excerpt) => (
					<blockquote
						key={excerpt}
						className="border-l-2 pl-2 text-xs text-muted-foreground"
					>
						{excerpt}
					</blockquote>
				))}
				{href === null ? null : (
					<a
						href={href}
						target="_blank"
						rel="noopener noreferrer"
						className="w-fit text-sm text-orange-600 hover:underline"
					>
						Voir l'annonce
					</a>
				)}
				{candidate.review == null ? (
					<div className="flex flex-wrap gap-2">
						<Button
							type="button"
							variant="outline"
							size="sm"
							disabled={pending}
							onClick={() => onReview(candidate, 'rightly_dropped')}
						>
							{dropReviewLabel('rightly_dropped')}
						</Button>
						<Button
							type="button"
							size="sm"
							disabled={pending}
							onClick={() => onReview(candidate, 'should_have_kept')}
						>
							{dropReviewLabel('should_have_kept')}
						</Button>
					</div>
				) : (
					<div className="flex flex-wrap items-center gap-3 text-sm">
						<Badge>{dropReviewLabel(candidate.review)}</Badge>
						{candidate.reviewed_at ? (
							<span className="text-xs text-muted-foreground">
								Revue le {formatDateTime(candidate.reviewed_at)}
							</span>
						) : null}
						{leadId === null ? null : (
							<a
								href={`/leads/${leadId}`}
								className="font-medium hover:underline"
								onClick={(event) => {
									event.preventDefault()
									onOpenLead(leadId)
								}}
							>
								Voir le lead
							</a>
						)}
					</div>
				)}
				{error === null ? null : (
					<p role="alert" className="text-xs text-destructive">
						{error}
					</p>
				)}
			</CardContent>
		</Card>
	)
}
