import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import {
	formatPoints,
	isValidThreshold,
	qualificationPoints,
	THRESHOLD_ERROR,
} from '#/pages/profiles/lib/scoring'
import type { Problem } from '#/pages/profiles/lib/types'

interface ThresholdFieldProps {
	value: number | null
	maxScore: number
	problems: readonly Problem[]
	onChange: (threshold: number | null) => void
}

const PATH = 'llm.qualification_threshold'

export function ThresholdField({
	value,
	maxScore,
	problems,
	onChange,
}: ThresholdFieldProps) {
	const messages = problems
		.filter((problem) => problem.path === PATH)
		.map((problem) => problem.message)
	if (!isValidThreshold(value) && messages.length === 0) {
		messages.push(THRESHOLD_ERROR)
	}
	const invalid = messages.length > 0
	const points = isValidThreshold(value)
		? formatPoints(qualificationPoints(value, maxScore))
		: null

	return (
		<div className="flex flex-col gap-1">
			<Label htmlFor="llm-qualification-threshold">
				Qualification threshold
			</Label>
			<div className="relative max-w-40">
				<Input
					id="llm-qualification-threshold"
					type="number"
					min={0}
					max={100}
					step={1}
					inputMode="numeric"
					className="pr-8"
					aria-invalid={invalid}
					aria-describedby="llm-qualification-threshold-help"
					value={value ?? ''}
					onChange={(event) => {
						const parsed = event.currentTarget.valueAsNumber
						onChange(Number.isNaN(parsed) ? null : parsed)
					}}
				/>
				<span
					aria-hidden="true"
					className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-sm text-muted-foreground"
				>
					%
				</span>
			</div>
			{points !== null && (
				<p
					id="llm-qualification-threshold-help"
					className="text-xs text-muted-foreground"
				>
					{`= ${points} points with the current factors (max ${formatPoints(maxScore)})`}
				</p>
			)}
			{messages.map((message) => (
				<p key={message} role="alert" className="text-xs text-destructive">
					{message}
				</p>
			))}
		</div>
	)
}
