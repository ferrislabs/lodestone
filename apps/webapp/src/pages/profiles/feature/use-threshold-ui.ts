import {
	isJsonObject,
	type JsonObject,
	removeIn,
	setIn,
} from '#/lib/schema-form'
import { readThreshold } from '#/pages/profiles/lib/scoring'

export function useThresholdUi(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
) {
	return {
		value: readThreshold(value),
		set: (threshold: number | null) => {
			const path = ['llm', 'qualification_threshold']
			const next =
				threshold === null
					? removeIn(value, path)
					: setIn(value, path, threshold)
			onChange(isJsonObject(next) ? next : value)
		},
	}
}
