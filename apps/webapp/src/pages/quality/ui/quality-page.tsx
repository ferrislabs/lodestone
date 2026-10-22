import { Label } from '#/components/ui/label'
import { Skeleton } from '#/components/ui/skeleton'
import { visibleProfiles } from '#/pages/quality/lib/calibration'
import { droppedReviewHref } from '#/pages/quality/lib/dropped'
import { hasDecisions } from '#/pages/quality/lib/format'
import { PERIOD_LABELS, PERIODS, type Period } from '#/pages/quality/lib/period'
import type { SuggestionOutcome } from '#/pages/quality/lib/sourcing'
import type {
	Calibration,
	ExclusionCandidate,
	KeywordSuggestion,
	ProfileSummary,
	Quality,
	TriageRecall,
} from '#/pages/quality/lib/types'
import { AgentsCard } from '#/pages/quality/ui/agents-card'
import { CalibrationCard } from '#/pages/quality/ui/calibration-card'
import { ConfidenceHistogram } from '#/pages/quality/ui/confidence-histogram'
import { DecisionsCard } from '#/pages/quality/ui/decisions-card'
import { DiscardedCard } from '#/pages/quality/ui/discarded-card'
import { ExclusionsCard } from '#/pages/quality/ui/exclusions-card'
import { KeywordSuggestionsCard } from '#/pages/quality/ui/keyword-suggestions-card'
import { RejectionReasonsCard } from '#/pages/quality/ui/rejection-reasons-card'
import { SourcingYieldCard } from '#/pages/quality/ui/sourcing-yield-card'
import { SuggestionsCard } from '#/pages/quality/ui/suggestions-card'
import { TriageRecallCard } from '#/pages/quality/ui/triage-recall-card'
import { VerdictPrecisionCard } from '#/pages/quality/ui/verdict-precision-card'
import { VersionsCard } from '#/pages/quality/ui/versions-card'

const SELECT_CLASS =
	'h-9 w-full rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'

const SKELETON_CARDS = ['a', 'b', 'c']

interface QualityPageProps {
	quality: Quality | null
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	profiles: readonly ProfileSummary[]
	profileId: string
	onProfileChange: (profileId: string) => void
	period: Period
	onPeriodChange: (period: Period) => void
	onOpenProfile: (profileId: string) => void
	copiedLeadId: string | null
	copyFailed: boolean
	onCopyExclusion: (candidate: ExclusionCandidate) => void
	calibration: Calibration | null
	calibrationRunning: boolean
	calibrationNotice: string | null
	calibrationError: string | null
	onRecalibrate: () => void
	onReviewTriage: (row: TriageRecall) => void
	onOpenDropped: (profileId: string) => void
	expandedAgents: readonly string[]
	onToggleAgent: (key: string) => void
	suggestions: readonly KeywordSuggestion[]
	suggestionOutcomes: Readonly<Record<string, SuggestionOutcome>>
	onApplySuggestion: (suggestion: KeywordSuggestion) => void
}

export function QualityPage({
	quality,
	state,
	errorMessage,
	profiles,
	profileId,
	onProfileChange,
	period,
	onPeriodChange,
	onOpenProfile,
	copiedLeadId,
	copyFailed,
	onCopyExclusion,
	calibration,
	calibrationRunning,
	calibrationNotice,
	calibrationError,
	onRecalibrate,
	onReviewTriage,
	onOpenDropped,
	expandedAgents,
	onToggleAgent,
	suggestions,
	suggestionOutcomes,
	onApplySuggestion,
}: QualityPageProps) {
	const calibrationCard =
		state === 'ready' && calibration !== null ? (
			<CalibrationCard
				calibration={calibration}
				profiles={visibleProfiles(calibration.profiles, profileId)}
				running={calibrationRunning}
				notice={calibrationNotice}
				errorMessage={calibrationError}
				onRecalculate={onRecalibrate}
			/>
		) : null
	const exclusionsCard =
		quality !== null &&
		(hasDecisions(quality) ||
			quality.exclusions.length > 0 ||
			quality.exclusion_candidates.length > 0) ? (
			<ExclusionsCard
				exclusions={quality.exclusions}
				candidates={quality.exclusion_candidates}
				copiedLeadId={copiedLeadId}
				copyFailed={copyFailed}
				onCopy={onCopyExclusion}
			/>
		) : null
	const triageCard =
		state === 'ready' &&
		quality !== null &&
		quality.triage_recall.length > 0 ? (
			<TriageRecallCard
				rows={quality.triage_recall}
				profiles={profiles}
				onReview={onReviewTriage}
			/>
		) : null
	const sourcingCards =
		state === 'ready' && quality !== null ? (
			<>
				{quality.sourcing_yield.length > 0 ? (
					<SourcingYieldCard
						agents={quality.sourcing_yield}
						profiles={profiles}
						expandedAgents={expandedAgents}
						onToggleAgent={onToggleAgent}
					/>
				) : null}
				{suggestions.length > 0 ? (
					<KeywordSuggestionsCard
						suggestions={suggestions}
						agents={quality.sourcing_yield}
						profiles={profiles}
						outcomes={suggestionOutcomes}
						onApply={onApplySuggestion}
					/>
				) : null}
			</>
		) : null
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<div className="flex flex-col gap-1">
				<h1 className="text-2xl font-bold tracking-tight">Qualité</h1>
				<p className="text-sm text-muted-foreground">
					Les décisions humaines (leads validés ou rejetés, brouillons approuvés
					ou rejetés) servent de référence pour mesurer la qualité des verdicts.
					Chaque décision garde l'état de la qualification au moment où elle a
					été prise.
				</p>
				<a
					href={droppedReviewHref(profileId)}
					className="w-fit text-sm font-medium text-orange-600 hover:underline"
					onClick={(event) => {
						event.preventDefault()
						onOpenDropped(profileId)
					}}
				>
					Revoir les annonces écartées
				</a>
			</div>
			<div className="flex flex-wrap items-end gap-x-8 gap-y-3">
				<fieldset className="flex flex-col gap-1.5">
					<legend className="text-sm font-medium">Période</legend>
					<div className="flex gap-1">
						{PERIODS.map((value) => (
							<label
								key={value}
								className="cursor-pointer rounded-md border px-3 py-1.5 text-sm has-checked:bg-primary has-checked:text-primary-foreground"
							>
								<input
									type="radio"
									name="period"
									value={value}
									checked={period === value}
									onChange={() => onPeriodChange(value)}
									className="sr-only"
								/>
								{PERIOD_LABELS[value]}
							</label>
						))}
					</div>
				</fieldset>
				<div className="flex w-full max-w-sm flex-col gap-1.5">
					<Label htmlFor="quality-profile">Profil</Label>
					<select
						id="quality-profile"
						className={SELECT_CLASS}
						value={profileId}
						onChange={(event) => onProfileChange(event.target.value)}
					>
						<option value="">Tous les profils</option>
						{profiles.map((profile) => (
							<option key={profile.id} value={profile.id}>
								{profile.name}
							</option>
						))}
					</select>
				</div>
			</div>
			{state === 'error' ? (
				<p role="alert" className="text-sm text-destructive">
					{errorMessage ?? 'Impossible de charger la qualité'}
				</p>
			) : null}
			{state === 'loading'
				? SKELETON_CARDS.map((id) => (
						<Skeleton key={id} className="h-32 w-full" />
					))
				: null}
			{state === 'ready' && quality !== null && !hasDecisions(quality) ? (
				<output className="rounded-xl border bg-card p-8 text-center text-sm text-muted-foreground">
					Aucune décision enregistrée sur cette période. Validez ou rejetez des
					leads, approuvez ou rejetez des brouillons : la qualité des verdicts
					sera mesurée ici.
				</output>
			) : null}
			{state === 'ready' && quality !== null && !hasDecisions(quality) ? (
				<>
					{exclusionsCard}
					{triageCard}
					{sourcingCards}
				</>
			) : null}
			{state === 'ready' && quality !== null && hasDecisions(quality) ? (
				<>
					<DecisionsCard decisions={quality.decisions} />
					<div className="grid gap-6 lg:grid-cols-2">
						<VerdictPrecisionCard verdicts={quality.verdicts} />
						<DiscardedCard discarded={quality.discarded} />
					</div>
					<ConfidenceHistogram buckets={quality.confidence} />
					<AgentsCard agents={quality.agents} />
					<VersionsCard versions={quality.versions} />
					<RejectionReasonsCard profiles={quality.profiles} />
					{exclusionsCard}
					{triageCard}
					{sourcingCards}
					<SuggestionsCard
						profiles={quality.profiles}
						onOpenProfile={onOpenProfile}
					/>
				</>
			) : null}
			{calibrationCard}
			{state === 'ready' && calibration === null && calibrationError ? (
				<p role="alert" className="text-sm text-destructive">
					{calibrationError}
				</p>
			) : null}
		</div>
	)
}
