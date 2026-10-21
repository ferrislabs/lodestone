import {
	appendIn,
	isJsonObject,
	type JsonObject,
	type JsonValue,
	removeIn,
	setIn,
} from '#/lib/schema-form'
import {
	DEFAULT_WEIGHT,
	type Factor,
	factorList,
	isAvailable,
	maxScore,
	nextSignal,
	readFactor,
	readThreshold,
	signalMeta,
	valueSuggestions,
} from '#/pages/profiles/lib/scoring'

function asObject(next: JsonValue, fallback: JsonObject): JsonObject {
	return isJsonObject(next) ? next : fallback
}

function focusWeight(index: number) {
	requestAnimationFrame(() => {
		document
			.querySelector<HTMLElement>(
				`[data-factor="${index}"] [role="radio"][tabindex="0"]`,
			)
			?.focus()
	})
}

export function useScoringUi(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
) {
	const factors: Factor[] = factorList(value).map(readFactor)
	const apply = (next: JsonValue) => onChange(asObject(next, value))

	return {
		factors,
		maxScore: maxScore(factors),
		threshold: readThreshold(value),
		suggestions: (kind: string) => valueSuggestions(value, kind),
		available: (kind: string) => isAvailable(value, kind),
		add: () => {
			apply(
				appendIn(value, ['scoring', 'factors'], {
					signal: nextSignal(value, factors),
					weight: DEFAULT_WEIGHT,
				}),
			)
			focusWeight(factors.length)
		},
		remove: (index: number) =>
			apply(removeIn(value, ['scoring', 'factors', index])),
		setSignal: (index: number, signal: string) => {
			const path = ['scoring', 'factors', index]
			const withSignal = setIn(value, [...path, 'signal'], signal)
			apply(
				signalMeta(signal)?.takesValue === true
					? withSignal
					: removeIn(withSignal, [...path, 'value']),
			)
		},
		setWeight: (index: number, weight: number | null) => {
			const path = ['scoring', 'factors', index, 'weight']
			apply(
				weight === null ? removeIn(value, path) : setIn(value, path, weight),
			)
		},
		setThreshold: (threshold: number | null) => {
			const path = ['llm', 'qualification_threshold']
			apply(
				threshold === null
					? removeIn(value, path)
					: setIn(value, path, threshold),
			)
		},
		setValue: (index: number, next: string) => {
			const path = ['scoring', 'factors', index, 'value']
			apply(next === '' ? removeIn(value, path) : setIn(value, path, next))
		},
	}
}
