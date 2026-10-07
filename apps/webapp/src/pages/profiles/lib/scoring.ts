import { isJsonObject, type JsonObject } from '#/lib/schema-form'

export const MIN_WEIGHT = 1
export const MAX_WEIGHT = 10
export const THRESHOLD_ERROR = 'Doit être un pourcentage de 0 à 100'

export function isValidThreshold(
	threshold: number | null,
): threshold is number {
	return (
		threshold !== null &&
		Number.isInteger(threshold) &&
		threshold >= 0 &&
		threshold <= 100
	)
}

export function readThreshold(definition: JsonObject): number | null {
	const llm = definition.llm
	if (!isJsonObject(llm)) return null
	return typeof llm.qualification_threshold === 'number'
		? llm.qualification_threshold
		: null
}
