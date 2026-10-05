import { NumberChoice } from '#/components/ui/number-choice'
import { MAX_WEIGHT, MIN_WEIGHT } from '#/pages/profiles/lib/scoring'

interface WeightPickerProps {
	labelledBy: string
	value: number | null
	invalid: boolean
	describedBy?: string
	onChange: (weight: number) => void
}

export function WeightPicker(props: WeightPickerProps) {
	return <NumberChoice {...props} min={MIN_WEIGHT} max={MAX_WEIGHT} />
}
