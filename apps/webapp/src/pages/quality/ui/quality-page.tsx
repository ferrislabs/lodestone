import { Label } from '#/components/ui/label'
import { Skeleton } from '#/components/ui/skeleton'
import { hasDecisions } from '#/pages/quality/lib/format'
import { PERIOD_LABELS, PERIODS, type Period } from '#/pages/quality/lib/period'
import type { ProfileSummary, Quality } from '#/pages/quality/lib/types'
import { AgentsCard } from '#/pages/quality/ui/agents-card'
import { ConfidenceHistogram } from '#/pages/quality/ui/confidence-histogram'
import { DecisionsCard } from '#/pages/quality/ui/decisions-card'
import { DiscardedCard } from '#/pages/quality/ui/discarded-card'
import { SuggestionsCard } from '#/pages/quality/ui/suggestions-card'
import { VerdictPrecisionCard } from '#/pages/quality/ui/verdict-precision-card'

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
}: QualityPageProps) {
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
			{state === 'ready' && quality !== null && hasDecisions(quality) ? (
				<>
					<DecisionsCard decisions={quality.decisions} />
					<div className="grid gap-6 lg:grid-cols-2">
						<VerdictPrecisionCard verdicts={quality.verdicts} />
						<DiscardedCard discarded={quality.discarded} />
					</div>
					<ConfidenceHistogram buckets={quality.confidence} />
					<AgentsCard agents={quality.agents} />
					<SuggestionsCard
						profiles={quality.profiles}
						onOpenProfile={onOpenProfile}
					/>
				</>
			) : null}
		</div>
	)
}
