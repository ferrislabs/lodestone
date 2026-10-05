import { patternOf } from '#/lib/schema-form/paths'
import type { FieldCopy, Segment, UiHints } from '#/lib/schema-form/types'

export function humanize(key: string): string {
	const spaced = key.replace(/_/g, ' ').trim()
	return spaced === '' ? spaced : spaced[0].toUpperCase() + spaced.slice(1)
}

export function copyFor(
	path: readonly Segment[],
	hints: UiHints,
): FieldCopy | undefined {
	return hints.fields[patternOf(path)]
}

export function labelFor(key: string, hints: UiHints): string {
	return hints.labels[key] ?? humanize(key)
}
