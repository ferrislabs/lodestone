import type { Segment } from '#/lib/schema-form/types'

export const ROOT_PATH = '$'

export function formatPath(segments: readonly Segment[]): string {
	let path = ''
	for (const segment of segments) {
		if (typeof segment === 'number') path += `[${segment}]`
		else path += path === '' ? segment : `.${segment}`
	}
	return path === '' ? ROOT_PATH : path
}

export function genericPath(segments: readonly Segment[]): string {
	return formatPath(segments.filter((segment) => typeof segment === 'string'))
}

export function patternOf(segments: readonly Segment[]): string {
	let path = ''
	for (const segment of segments) {
		if (typeof segment === 'number') path += '[]'
		else path += path === '' ? segment : `.${segment}`
	}
	return path
}

export function fieldId(segments: readonly Segment[]): string {
	return `field-${formatPath(segments).replace(/[^A-Za-z0-9]+/g, '-')}`
}
