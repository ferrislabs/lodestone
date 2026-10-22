import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Label } from '#/components/ui/label'
import { Skeleton } from '#/components/ui/skeleton'
import {
	DROP_REASONS,
	type DroppedEntry,
	dropReasonLabel,
	REVIEW_STATE_LABELS,
	REVIEW_STATES,
	type ReviewState,
} from '#/pages/quality/lib/dropped'
import type {
	DroppedCandidate,
	DropReason,
	DropReview,
	ProfileSummary,
} from '#/pages/quality/lib/types'
import { DroppedCard } from '#/pages/quality/ui/dropped-card'

const SELECT_CLASS =
	'h-9 w-full rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'

const SKELETON_CARDS = ['a', 'b', 'c']

interface DroppedPageProps {
	profiles: readonly ProfileSummary[]
	profileId: string
	onProfileChange: (profileId: string) => void
	reason: DropReason | ''
	onReasonChange: (reason: DropReason | '') => void
	agent: string | null
	onClearAgent: () => void
	reviewState: ReviewState
	onReviewStateChange: (state: ReviewState) => void
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	entries: readonly DroppedEntry[]
	refreshing: boolean
	onRefresh: () => void
	onReview: (candidate: DroppedCandidate, review: DropReview) => void
	onOpenLead: (leadId: string) => void
}

function isReason(value: string): value is DropReason {
	return DROP_REASONS.some((reason) => reason === value)
}

export function DroppedPage({
	profiles,
	profileId,
	onProfileChange,
	reason,
	onReasonChange,
	agent,
	onClearAgent,
	reviewState,
	onReviewStateChange,
	state,
	errorMessage,
	entries,
	refreshing,
	onRefresh,
	onReview,
	onOpenLead,
}: DroppedPageProps) {
	const chosen = profileId !== ''
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<div className="flex flex-col gap-1">
				<h1 className="text-2xl font-bold tracking-tight">
					Annonces écartées au tri
				</h1>
				<p className="text-sm text-muted-foreground">
					Un échantillon aléatoire des annonces que le tri par mots-clés a
					écartées. Dites si chacune l'était à raison : la part d'annonces qui
					auraient dû être gardées mesure le rappel du tri.
				</p>
			</div>
			<div className="flex flex-wrap items-end gap-x-8 gap-y-3">
				<div className="flex w-full max-w-xs flex-col gap-1.5">
					<Label htmlFor="dropped-profile">Profil</Label>
					<select
						id="dropped-profile"
						className={SELECT_CLASS}
						value={profileId}
						onChange={(event) => onProfileChange(event.target.value)}
					>
						<option value="">Choisir un profil</option>
						{profiles.map((profile) => (
							<option key={profile.id} value={profile.id}>
								{profile.name}
							</option>
						))}
					</select>
				</div>
				<div className="flex w-full max-w-xs flex-col gap-1.5">
					<Label htmlFor="dropped-reason">Raison</Label>
					<select
						id="dropped-reason"
						className={SELECT_CLASS}
						value={reason}
						onChange={(event) =>
							onReasonChange(
								isReason(event.target.value) ? event.target.value : '',
							)
						}
					>
						<option value="">Toutes les raisons</option>
						{DROP_REASONS.map((value) => (
							<option key={value} value={value}>
								{dropReasonLabel(value)}
							</option>
						))}
					</select>
				</div>
				<fieldset className="flex flex-col gap-1.5">
					<legend className="text-sm font-medium">État</legend>
					<div className="flex gap-1">
						{REVIEW_STATES.map((value) => (
							<label
								key={value}
								className="cursor-pointer rounded-md border px-3 py-1.5 text-sm has-checked:bg-primary has-checked:text-primary-foreground"
							>
								<input
									type="radio"
									name="dropped-state"
									value={value}
									checked={reviewState === value}
									onChange={() => onReviewStateChange(value)}
									className="sr-only"
								/>
								{REVIEW_STATE_LABELS[value]}
							</label>
						))}
					</div>
				</fieldset>
				<Button
					type="button"
					variant="outline"
					disabled={!chosen || refreshing}
					onClick={onRefresh}
				>
					Nouvel échantillon
				</Button>
			</div>
			{agent === null ? null : (
				<div className="flex items-center gap-2 text-sm">
					<Badge variant="secondary">Agent : {agent}</Badge>
					<Button type="button" variant="link" size="sm" onClick={onClearAgent}>
						Retirer le filtre agent
					</Button>
				</div>
			)}
			{chosen ? null : (
				<output className="rounded-xl border bg-card p-8 text-center text-sm text-muted-foreground">
					Choisissez un profil pour revoir un échantillon des annonces écartées
					par le tri.
				</output>
			)}
			{chosen && state === 'error' ? (
				<p role="alert" className="text-sm text-destructive">
					{errorMessage ?? 'Impossible de charger les annonces écartées'}
				</p>
			) : null}
			{chosen && state === 'loading'
				? SKELETON_CARDS.map((id) => (
						<Skeleton key={id} className="h-32 w-full" />
					))
				: null}
			{chosen && state === 'ready' && entries.length === 0 ? (
				<output className="rounded-xl border bg-card p-8 text-center text-sm text-muted-foreground">
					Aucune annonce écartée pour ces critères.
				</output>
			) : null}
			{chosen && state === 'ready' && entries.length > 0 ? (
				<ul aria-label="Annonces écartées" className="flex flex-col gap-4">
					{entries.map((entry) => (
						<li key={entry.candidate.id}>
							<DroppedCard
								entry={entry}
								onReview={onReview}
								onOpenLead={onOpenLead}
							/>
						</li>
					))}
				</ul>
			) : null}
		</div>
	)
}
