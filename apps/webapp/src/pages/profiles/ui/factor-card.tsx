import { Trash2Icon } from 'lucide-react'

import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { pluralize } from '#/lib/labels'
import {
	DISTINCT_SIGNALS,
	type Factor,
	factorTitle,
	isValidWeight,
	SIGNAL_GROUPS,
	SIGNAL_KINDS,
	signalMeta,
	WEIGHT_ERROR,
} from '#/pages/profiles/lib/scoring'
import type { Problem } from '#/pages/profiles/lib/types'
import { WeightPicker } from '#/pages/profiles/ui/weight-picker'

interface FactorCardProps {
	index: number
	factor: Factor
	suggestions: readonly string[]
	available: boolean
	problems: readonly Problem[]
	onSignal: (signal: string) => void
	onWeight: (weight: number) => void
	onValue: (value: string) => void
	onRemove: () => void
}

function pathProblems(problems: readonly Problem[], path: string): Problem[] {
	return problems.filter((problem) => problem.path === path)
}

function Errors({ id, items }: { id: string; items: readonly Problem[] }) {
	if (items.length === 0) return null
	return (
		<div id={id} className="flex flex-col gap-0.5">
			{items.map((problem) => (
				<p
					key={problem.message}
					role="alert"
					className="text-xs text-destructive"
				>
					{problem.message}
				</p>
			))}
		</div>
	)
}

export function FactorCard({
	index,
	factor,
	suggestions,
	available,
	problems,
	onSignal,
	onWeight,
	onValue,
	onRemove,
}: FactorCardProps) {
	const base = `scoring.factors[${index}]`
	const meta = signalMeta(factor.signal)
	const title = factorTitle(factor)
	const signalProblems = pathProblems(problems, `${base}.signal`)
	const serverWeightProblems = pathProblems(problems, `${base}.weight`)
	const weightProblems =
		isValidWeight(factor.weight) || serverWeightProblems.length > 0
			? serverWeightProblems
			: [
					...serverWeightProblems,
					{ path: `${base}.weight`, message: WEIGHT_ERROR },
				]
	const valueProblems = pathProblems(problems, `${base}.value`)
	const failing =
		signalProblems.length + weightProblems.length + valueProblems.length > 0
	const weightId = `factor-weight-${index}`
	const signalId = `factor-signal-${index}`
	const valueId = `factor-value-${index}`
	const listId = `factor-values-${index}`
	const helper = !isValidWeight(factor.weight)
		? null
		: factor.signal === DISTINCT_SIGNALS
			? `${factor.weight} ${pluralize(factor.weight, 'point', 'points')} par type de signal distinct`
			: `Ajoute jusqu'à ${factor.weight} ${pluralize(factor.weight, 'point', 'points')}`

	return (
		<li
			data-factor={index}
			className={`flex flex-col gap-3 rounded-xl border bg-card p-4 shadow-sm ${
				failing ? 'border-destructive' : ''
			}`}
		>
			<div className="flex items-start justify-between gap-2">
				<h3 className="text-sm font-semibold">{title}</h3>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					aria-label={`Supprimer le facteur ${title}`}
					onClick={onRemove}
				>
					<Trash2Icon />
				</Button>
			</div>
			<div className="flex flex-col gap-1">
				<Label id={weightId}>Poids</Label>
				<WeightPicker
					labelledBy={weightId}
					value={factor.weight}
					invalid={weightProblems.length > 0}
					describedBy={`${weightId}-errors`}
					onChange={onWeight}
				/>
				{helper !== null && (
					<p className="text-xs text-muted-foreground">{helper}</p>
				)}
				<Errors id={`${weightId}-errors`} items={weightProblems} />
			</div>
			<div className="flex flex-col gap-1">
				<Label htmlFor={signalId}>Signal</Label>
				<select
					id={signalId}
					className="h-9 w-full rounded-md border bg-background px-2 text-sm"
					value={factor.signal}
					aria-invalid={signalProblems.length > 0}
					onChange={(event) => onSignal(event.currentTarget.value)}
				>
					{meta === null && (
						<option value={factor.signal}>
							{factor.signal === '' ? 'Choisir un signal' : factor.signal}
						</option>
					)}
					{SIGNAL_GROUPS.map((group) => (
						<optgroup key={group} label={group}>
							{SIGNAL_KINDS.filter((item) => item.group === group).map(
								(item) => (
									<option key={item.kind} value={item.kind}>
										{item.label}
									</option>
								),
							)}
						</optgroup>
					))}
				</select>
				{meta !== null && (
					<p className="text-xs text-muted-foreground">{meta.description}</p>
				)}
				<Errors id={`${signalId}-errors`} items={signalProblems} />
			</div>
			{meta?.takesValue === true && (
				<div className="flex flex-col gap-1">
					<Label htmlFor={valueId}>Valeur</Label>
					<Input
						id={valueId}
						list={listId}
						placeholder="Toute valeur"
						value={factor.value}
						aria-invalid={valueProblems.length > 0}
						onChange={(event) => onValue(event.currentTarget.value)}
					/>
					<datalist id={listId} data-testid={listId}>
						{suggestions.map((suggestion) => (
							<option key={suggestion} value={suggestion} />
						))}
					</datalist>
					<Errors id={`${valueId}-errors`} items={valueProblems} />
				</div>
			)}
			{!available && (
				<p className="text-xs text-muted-foreground">
					Aucun agent n'émet encore ce signal
				</p>
			)}
		</li>
	)
}
