import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import {
	BUDGET_CENTS_MAX,
	BUDGET_CENTS_MIN,
	SAMPLE_SIZE_MAX,
	SAMPLE_SIZE_MIN,
} from '#/pages/profiles/lib/trials'

interface TrialLauncherProps {
	sampleSize: string
	budgetCents: string
	busy: boolean
	message: string | null
	onSampleSize: (value: string) => void
	onBudgetCents: (value: string) => void
	onLaunch: () => void
	onCancel: () => void
}

export function TrialLauncher({
	sampleSize,
	budgetCents,
	busy,
	message,
	onSampleSize,
	onBudgetCents,
	onLaunch,
	onCancel,
}: TrialLauncherProps) {
	const launchOnEnter = (event: React.KeyboardEvent<HTMLInputElement>) => {
		if (event.key !== 'Enter') return
		event.preventDefault()
		if (!busy) onLaunch()
	}
	return (
		<fieldset
			aria-label="Essai sur les leads décidés"
			className="flex min-w-0 flex-col gap-3 rounded-md border bg-muted/30 p-3"
		>
			<p className="text-sm text-muted-foreground">
				Les agents du brouillon sont lancés sur des leads déjà validés ou
				rejetés. Rien n'est écrit sur ces leads ; les appels comptent dans le
				budget.
			</p>
			<div className="flex flex-wrap items-end gap-3">
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="trial-sample-size">Taille de l'échantillon</Label>
					<Input
						id="trial-sample-size"
						type="number"
						inputMode="numeric"
						min={SAMPLE_SIZE_MIN}
						max={SAMPLE_SIZE_MAX}
						step={1}
						className="w-40"
						value={sampleSize}
						disabled={busy}
						onChange={(event) => onSampleSize(event.target.value)}
						onKeyDown={launchOnEnter}
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="trial-budget-cents">Budget (centimes)</Label>
					<Input
						id="trial-budget-cents"
						type="number"
						inputMode="numeric"
						min={BUDGET_CENTS_MIN}
						max={BUDGET_CENTS_MAX}
						step={1}
						className="w-40"
						value={budgetCents}
						disabled={busy}
						onChange={(event) => onBudgetCents(event.target.value)}
						onKeyDown={launchOnEnter}
					/>
				</div>
				<Button type="button" size="sm" disabled={busy} onClick={onLaunch}>
					{busy ? 'Lancement…' : "Lancer l'essai"}
				</Button>
				<Button
					type="button"
					size="sm"
					variant="ghost"
					disabled={busy}
					onClick={onCancel}
				>
					Annuler
				</Button>
			</div>
			{message ? (
				<p role="alert" className="text-sm text-destructive">
					{message}
				</p>
			) : null}
		</fieldset>
	)
}
