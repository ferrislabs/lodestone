import type { KeyboardEvent } from 'react'

interface NumberChoiceProps {
	labelledBy?: string
	label?: string
	min: number
	max: number
	value: number | null
	invalid: boolean
	describedBy?: string
	onChange: (value: number) => void
}

function stepped(
	key: string,
	value: number | null,
	min: number,
	max: number,
): number | null {
	if (key === 'Home') return min
	if (key === 'End') return max
	const forward = key === 'ArrowRight' || key === 'ArrowUp'
	const backward = key === 'ArrowLeft' || key === 'ArrowDown'
	if (!forward && !backward) return null
	if (value === null) return forward ? min : max
	return Math.min(max, Math.max(min, value + (forward ? 1 : -1)))
}

export function NumberChoice({
	labelledBy,
	label,
	min,
	max,
	value,
	invalid,
	describedBy,
	onChange,
}: NumberChoiceProps) {
	const choices = Array.from({ length: max - min + 1 }, (_, i) => min + i)
	const selected = value !== null && choices.includes(value) ? value : null
	const tabbable = selected ?? min

	const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
		const next = stepped(event.key, selected, min, max)
		if (next === null) return
		event.preventDefault()
		onChange(next)
		event.currentTarget
			.querySelector<HTMLElement>(`[data-choice="${next}"]`)
			?.focus()
	}

	return (
		<div
			role="radiogroup"
			aria-labelledby={labelledBy}
			aria-label={label}
			aria-invalid={invalid}
			aria-describedby={describedBy}
			className={`flex w-full gap-1 rounded-md ${
				invalid ? 'ring-1 ring-destructive' : ''
			}`}
			onKeyDown={onKeyDown}
		>
			{choices.map((choice) => {
				const checked = choice === selected
				return (
					// biome-ignore lint/a11y/useSemanticElements: segmented control, native radios cannot be styled as joined buttons
					<button
						key={choice}
						type="button"
						role="radio"
						aria-checked={checked}
						tabIndex={choice === tabbable ? 0 : -1}
						data-choice={choice}
						className={`h-8 min-w-0 flex-1 rounded-md border text-sm font-medium outline-none transition-colors focus-visible:ring-[3px] focus-visible:ring-ring/50 ${
							checked
								? 'border-primary bg-primary text-primary-foreground'
								: 'bg-background hover:bg-accent hover:text-accent-foreground'
						}`}
						onClick={() => onChange(choice)}
					>
						{choice}
					</button>
				)
			})}
		</div>
	)
}
