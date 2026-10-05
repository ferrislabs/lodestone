import type { UiHints } from '#/lib/schema-form/types'

export function humanize(key: string): string {
	const spaced = key.replace(/_/g, ' ').trim()
	return spaced === '' ? spaced : spaced[0].toUpperCase() + spaced.slice(1)
}

export function labelFor(key: string, hints: UiHints): string {
	return hints.labels[key] ?? humanize(key)
}
