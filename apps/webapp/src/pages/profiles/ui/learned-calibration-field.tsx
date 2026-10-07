import { Label } from '#/components/ui/label'
import { Switch } from '#/components/ui/switch'
import { LEARNED_CALIBRATION_COPY } from '#/pages/profiles/lib/field-copy'

interface LearnedCalibrationFieldProps {
	id: string
	learned: boolean
	lines: readonly string[]
	onChange: (learned: boolean) => void
}

export function LearnedCalibrationField({
	id,
	learned,
	lines,
	onChange,
}: LearnedCalibrationFieldProps) {
	return (
		<div className="flex flex-col gap-1.5">
			<div className="flex items-center gap-2">
				<Switch id={id} checked={learned} onCheckedChange={onChange} />
				<Label htmlFor={id}>{LEARNED_CALIBRATION_COPY.label}</Label>
			</div>
			<p className="text-xs text-muted-foreground">
				{LEARNED_CALIBRATION_COPY.description}
			</p>
			{lines.length > 0 ? (
				<ul
					aria-label="État de la calibration"
					className="flex flex-col gap-0.5 text-xs"
				>
					{lines.map((line) => (
						<li key={line}>{line}</li>
					))}
				</ul>
			) : null}
		</div>
	)
}
