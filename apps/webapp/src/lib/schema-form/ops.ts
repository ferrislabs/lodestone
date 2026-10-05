import type { JsonValue, Segment } from '#/lib/schema-form/types'

export function getIn(
	value: JsonValue | undefined,
	segments: readonly Segment[],
): JsonValue | undefined {
	let current = value
	for (const segment of segments) {
		if (typeof segment === 'number') {
			if (!Array.isArray(current)) return undefined
			current = current[segment]
		} else {
			if (
				typeof current !== 'object' ||
				current === null ||
				Array.isArray(current)
			) {
				return undefined
			}
			current = current[segment]
		}
	}
	return current
}

export function setIn(
	value: JsonValue,
	segments: readonly Segment[],
	next: JsonValue,
): JsonValue {
	const [head, ...rest] = segments
	if (head === undefined) return next
	if (typeof head === 'number') {
		const list = Array.isArray(value) ? value : []
		const copy = [...list]
		copy[head] = setIn(list[head] ?? null, rest, next)
		return copy
	}
	const record =
		typeof value === 'object' && value !== null && !Array.isArray(value)
			? value
			: {}
	return { ...record, [head]: setIn(record[head] ?? null, rest, next) }
}

export function removeIn(
	value: JsonValue,
	segments: readonly Segment[],
): JsonValue {
	const [head, ...rest] = segments
	if (head === undefined) return value
	if (typeof head === 'number') {
		if (!Array.isArray(value)) return value
		if (rest.length === 0) return value.filter((_, index) => index !== head)
		return value.map((item, index) =>
			index === head ? removeIn(item, rest) : item,
		)
	}
	if (typeof value !== 'object' || value === null || Array.isArray(value)) {
		return value
	}
	if (rest.length === 0) {
		return Object.fromEntries(
			Object.entries(value).filter(([key]) => key !== head),
		)
	}
	const child = value[head]
	if (child === undefined) return value
	return { ...value, [head]: removeIn(child, rest) }
}

export function appendIn(
	value: JsonValue,
	segments: readonly Segment[],
	item: JsonValue,
): JsonValue {
	const current = getIn(value, segments)
	const list = Array.isArray(current) ? current : []
	return setIn(value, segments, [...list, item])
}

export function stableStringify(value: JsonValue): string {
	return JSON.stringify(value)
}
