import {
	classify,
	isOptionalProperty,
	propertiesOf,
	unwrapNullable,
} from '#/lib/schema-form/classify'
import { genericPath } from '#/lib/schema-form/paths'
import type {
	JsonObject,
	JsonValue,
	Schema,
	Segment,
	UiHints,
} from '#/lib/schema-form/types'

export function defaultValue(
	root: Schema,
	schema: Schema,
	hints: UiHints,
	path: readonly Segment[] = [],
): JsonValue {
	const hinted = hints.defaults[genericPath(path)]
	if (hinted !== undefined) return hinted
	const node = classify(root, schema)
	if (node.schema.default !== undefined && node.kind !== 'object') {
		return node.schema.default
	}
	switch (node.kind) {
		case 'const':
			return node.value
		case 'enum':
			return node.options[0] ?? null
		case 'nullable':
			return defaultValue(root, node.inner, hints, path)
		case 'union': {
			const first = node.variants[0]
			return first ? defaultValue(root, first.schema, hints, path) : null
		}
		case 'object': {
			const entries: [string, JsonValue][] = []
			for (const property of propertiesOf(node.schema)) {
				if (isOptionalProperty(root, property)) continue
				entries.push([
					property.key,
					defaultValue(root, property.schema, hints, [...path, property.key]),
				])
			}
			const base: JsonObject = Object.fromEntries(entries)
			return base
		}
		case 'array':
			return []
		case 'string':
			return ''
		case 'number':
			return node.schema.minimum ?? 0
		case 'boolean':
			return false
		case 'unknown':
			return null
	}
}

export function enabledValue(
	root: Schema,
	schema: Schema,
	hints: UiHints,
	path: readonly Segment[],
): JsonValue {
	return defaultValue(root, unwrapNullable(root, schema), hints, path)
}
