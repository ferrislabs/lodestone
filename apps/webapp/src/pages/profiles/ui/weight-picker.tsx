import type { KeyboardEvent } from 'react'

import { MAX_WEIGHT, MIN_WEIGHT, WEIGHTS } from '#/pages/profiles/lib/scoring'

interface WeightPickerProps {
	labelledBy: string
	value: number | null
	invalid: boolean
	describedBy?: string
	onChange: (weight: number) => void
}

function stepped(key: string, value: number | null): number | null {
	if (key === 'Home') return MIN_WEIGHT
	if (key === 'End') return MAX_WEIGHT
	const forward = key === 'ArrowRight' || key === 'ArrowUp'
	const backward = key === 'ArrowLeft' || key === 'ArrowDown'
	if (!forward && !backward) return null
	if (value === null) return forward ? MIN_WEIGHT : MAX_WEIGHT
	return Math.min(MAX_WEIGHT, Math.max(MIN_WEIGHT, value + (forward ? 1 : -1)))
}

export function WeightPicker({
	labelledBy,
	value,
	invalid,
	describedBy,
	onChange,
}: WeightPickerProps) {
	const selected = value !== null && WEIGHTS.includes(value) ? value : null
	const tabbable = selected ?? MIN_WEIGHT

	const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
		const next = stepped(event.key, selected)
		if (next === null) return
		event.preventDefault()
		onChange(next)
		event.currentTarget
			.querySelector<HTMLElement>(`[data-weight="${next}"]`)
			?.focus()
	}

	return (
		<div
			role="radiogroup"
			aria-labelledby={labelledBy}
			aria-invalid={invalid}
			aria-describedby={describedBy}
			className={`flex w-full gap-1 rounded-md ${
				invalid ? 'ring-1 ring-destructive' : ''
			}`}
			onKeyDown={onKeyDown}
		>
			{WEIGHTS.map((weight) => {
				const checked = weight === selected
				return (
					// biome-ignore lint/a11y/useSemanticElements: segmented control, native radios cannot be styled as joined buttons
					<button
						key={weight}
						type="button"
						role="radio"
						aria-checked={checked}
						tabIndex={weight === tabbable ? 0 : -1}
						data-weight={weight}
						className={`h-8 min-w-0 flex-1 rounded-md border text-sm font-medium outline-none transition-colors focus-visible:ring-[3px] focus-visible:ring-ring/50 ${
							checked
								? 'border-primary bg-primary text-primary-foreground'
								: 'bg-background hover:bg-accent hover:text-accent-foreground'
						}`}
						onClick={() => onChange(weight)}
					>
						{weight}
					</button>
				)
			})}
		</div>
	)
}
