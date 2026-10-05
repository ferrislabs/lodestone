import { Button } from '#/components/ui/button'
import { Label } from '#/components/ui/label'
import { NO_PROVIDER } from '#/pages/llm/lib/tiers'
import {
	type Provider,
	TIERS,
	type TierName,
	type TierSelection,
} from '#/pages/llm/lib/types'

const SELECT_CLASS =
	'h-9 w-full rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'

interface TierPanelProps {
	providers: readonly Provider[]
	selection: TierSelection
	changed: boolean
	applying: boolean
	error: string | null
	onSelect: (tier: TierName, providerId: string) => void
	onApply: () => void
}

export function TierPanel({
	providers,
	selection,
	changed,
	applying,
	error,
	onSelect,
	onApply,
}: TierPanelProps) {
	return (
		<div className="flex flex-col gap-3 rounded-xl border bg-card p-4">
			<h2 className="text-sm font-semibold">Tier assignment</h2>
			<div className="grid gap-3 md:grid-cols-2">
				{TIERS.map((tier) => (
					<div key={tier} className="flex flex-col gap-1.5">
						<Label htmlFor={`tier-${tier}`} className="capitalize">
							{tier}
						</Label>
						<select
							id={`tier-${tier}`}
							className={SELECT_CLASS}
							value={selection[tier]}
							onChange={(event) => onSelect(tier, event.target.value)}
						>
							<option value={NO_PROVIDER}>None</option>
							{providers.map((provider) => (
								<option
									key={provider.id}
									value={provider.id}
									disabled={!provider.enabled}
								>
									{provider.name}
									{provider.enabled ? '' : ' (disabled)'}
								</option>
							))}
						</select>
					</div>
				))}
			</div>
			{error ? (
				<p role="alert" className="text-sm text-destructive">
					{error}
				</p>
			) : null}
			<div>
				<Button size="sm" disabled={!changed || applying} onClick={onApply}>
					{applying ? 'Applying...' : 'Apply tiers'}
				</Button>
			</div>
		</div>
	)
}
