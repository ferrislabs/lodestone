import {
	appendIn,
	isJsonObject,
	type JsonObject,
	type JsonValue,
	removeIn,
	setIn,
} from '#/lib/schema-form'
import {
	type Factor,
	factorList,
	isAvailable,
	maxWeight,
	nextSignal,
	readFactor,
	signalMeta,
	valueSuggestions,
} from '#/pages/profiles/lib/scoring'

function asObject(next: JsonValue, fallback: JsonObject): JsonObject {
	return isJsonObject(next) ? next : fallback
}

function focusWeight(index: number) {
	requestAnimationFrame(() => {
		document.getElementById(`factor-weight-${index}`)?.focus()
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
		maxWeight: maxWeight(factors),
		suggestions: (kind: string) => valueSuggestions(value, kind),
		available: (kind: string) => isAvailable(value, kind),
		add: () => {
			apply(
				appendIn(value, ['scoring', 'factors'], {
					signal: nextSignal(value, factors),
					weight: 10,
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
		setWeight: (index: number, weight: number) =>
			apply(setIn(value, ['scoring', 'factors', index, 'weight'], weight)),
		setValue: (index: number, next: string) => {
			const path = ['scoring', 'factors', index, 'value']
			apply(next === '' ? removeIn(value, path) : setIn(value, path, next))
		},
	}
}
