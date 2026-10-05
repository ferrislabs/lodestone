import { PlusIcon } from 'lucide-react'
import type { Factor } from '#/pages/profiles/lib/scoring'
import type { Problem } from '#/pages/profiles/lib/types'
import { FactorCard } from '#/pages/profiles/ui/factor-card'

interface ScoringGridProps {
	factors: readonly Factor[]
	problems: readonly Problem[]
	suggestions: (kind: string) => readonly string[]
	available: (kind: string) => boolean
	maxWeight: number
	onAdd: () => void
	onRemove: (index: number) => void
	onSignal: (index: number, signal: string) => void
	onWeight: (index: number, weight: number) => void
	onValue: (index: number, value: string) => void
}

export function ScoringGrid({
	factors,
	problems,
	suggestions,
	available,
	maxWeight,
	onAdd,
	onRemove,
	onSignal,
	onWeight,
	onValue,
}: ScoringGridProps) {
	return (
		<section aria-label="Scoring factors" className="flex flex-col gap-3">
			<p className="text-sm text-muted-foreground">
				{factors.length === 0
					? 'No scoring factor. Leads will all score 0.'
					: `${factors.length} ${factors.length === 1 ? 'factor' : 'factors'} · max weight ${maxWeight}`}
			</p>
			<ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
				{factors.map((factor, index) => (
					<FactorCard
						// biome-ignore lint/suspicious/noArrayIndexKey: factors have no identity
						key={index}
						index={index}
						factor={factor}
						suggestions={suggestions(factor.signal)}
						available={available(factor.signal)}
						problems={problems}
						onSignal={(signal) => onSignal(index, signal)}
						onWeight={(weight) => onWeight(index, weight)}
						onValue={(value) => onValue(index, value)}
						onRemove={() => onRemove(index)}
					/>
				))}
				<li>
					<button
						type="button"
						className="flex h-full min-h-40 w-full flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed text-sm text-muted-foreground hover:border-primary hover:text-foreground"
						onClick={onAdd}
					>
						<PlusIcon className="size-5" />
						Add factor
					</button>
				</li>
			</ul>
		</section>
	)
}
