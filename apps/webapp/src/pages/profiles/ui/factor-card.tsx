import { Trash2Icon } from 'lucide-react'

import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import {
	DISTINCT_SIGNALS,
	type Factor,
	factorTitle,
	SIGNAL_GROUPS,
	SIGNAL_KINDS,
	signalMeta,
} from '#/pages/profiles/lib/scoring'
import type { Problem } from '#/pages/profiles/lib/types'

interface FactorCardProps {
	index: number
	factor: Factor
	suggestions: readonly string[]
	available: boolean
	problems: readonly Problem[]
	onSignal: (signal: string) => void
	onWeight: (weight: number | null) => void
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
	const weightProblems = pathProblems(problems, `${base}.weight`)
	const valueProblems = pathProblems(problems, `${base}.value`)
	const failing =
		signalProblems.length + weightProblems.length + valueProblems.length > 0
	const weightId = `factor-weight-${index}`
	const signalId = `factor-signal-${index}`
	const valueId = `factor-value-${index}`
	const listId = `factor-values-${index}`
	const helper =
		factor.signal === DISTINCT_SIGNALS
			? 'points per distinct signal kind'
			: 'points per unit of strength'

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
					aria-label={`Remove factor ${title}`}
					onClick={onRemove}
				>
					<Trash2Icon />
				</Button>
			</div>
			<div className="flex flex-col gap-1">
				<Label htmlFor={weightId}>Weight</Label>
				<Input
					id={weightId}
					type="number"
					step={1}
					className="h-14 text-center text-3xl font-semibold"
					value={factor.weight ?? ''}
					aria-invalid={weightProblems.length > 0}
					onChange={(event) => {
						const parsed = event.currentTarget.valueAsNumber
						onWeight(Number.isNaN(parsed) ? null : parsed)
					}}
				/>
				<p className="text-xs text-muted-foreground">{helper}</p>
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
							{factor.signal === '' ? 'Choose a signal' : factor.signal}
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
					<Label htmlFor={valueId}>Value</Label>
					<Input
						id={valueId}
						list={listId}
						placeholder="Any value"
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
					No agent emits this signal yet
				</p>
			)}
		</li>
	)
}
