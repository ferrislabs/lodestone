import {
	classify,
	propertiesOf,
	variantIndex,
} from '#/lib/schema-form/classify'
import { formatPath } from '#/lib/schema-form/paths'
import type {
	JsonValue,
	Problem,
	Schema,
	Segment,
} from '#/lib/schema-form/types'

function problem(path: readonly Segment[], message: string): Problem {
	return { path: formatPath(path), message }
}

function isPresent(value: JsonValue | undefined): value is JsonValue {
	return value !== undefined && value !== null
}

export function validateValue(
	root: Schema,
	schema: Schema,
	value: JsonValue | undefined,
	path: readonly Segment[] = [],
): Problem[] {
	const node = classify(root, schema)
	switch (node.kind) {
		case 'nullable':
			return isPresent(value)
				? validateValue(root, node.inner, value, path)
				: []
		case 'string':
			if (typeof value !== 'string') return [problem(path, 'is required')]
			return value.trim() === '' ? [problem(path, 'must not be empty')] : []
		case 'number': {
			if (typeof value !== 'number' || Number.isNaN(value)) {
				return [problem(path, 'must be a number')]
			}
			const { minimum, maximum } = node.schema
			if (minimum !== undefined && value < minimum) {
				return [problem(path, `must be at least ${minimum}`)]
			}
			if (maximum !== undefined && value > maximum) {
				return [problem(path, `must be at most ${maximum}`)]
			}
			if (node.integer && !Number.isInteger(value)) {
				return [problem(path, 'must be a whole number')]
			}
			return []
		}
		case 'array': {
			if (!Array.isArray(value)) return []
			return value.flatMap((item, index) =>
				validateValue(root, node.items, item, [...path, index]),
			)
		}
		case 'union': {
			if (!isPresent(value)) return []
			const variant = node.variants[variantIndex(node.variants, value)]
			return variant ? validateValue(root, variant.schema, value, path) : []
		}
		case 'object': {
			if (typeof value !== 'object' || value === null || Array.isArray(value)) {
				return [problem(path, 'is required')]
			}
			return propertiesOf(node.schema).flatMap((property) => {
				const child = value[property.key]
				const childPath = [...path, property.key]
				if (child === undefined || child === null) {
					const missing =
						property.required &&
						!property.hasDefault &&
						classify(root, property.schema).kind !== 'nullable'
					return missing ? [problem(childPath, 'is required')] : []
				}
				return validateValue(root, property.schema, child, childPath)
			})
		}
		case 'const':
		case 'enum':
		case 'boolean':
		case 'unknown':
			return []
	}
}

export function fieldPaths(
	root: Schema,
	schema: Schema,
	value: JsonValue | undefined,
	path: readonly Segment[] = [],
): Set<string> {
	const found = new Set<string>([formatPath(path)])
	const node = classify(root, schema)
	const merge = (other: Set<string>) => {
		for (const item of other) found.add(item)
	}
	switch (node.kind) {
		case 'nullable':
			if (isPresent(value)) merge(fieldPaths(root, node.inner, value, path))
			break
		case 'array':
			if (Array.isArray(value)) {
				value.forEach((item, index) => {
					merge(fieldPaths(root, node.items, item, [...path, index]))
				})
			}
			break
		case 'union': {
			const variant = node.variants[variantIndex(node.variants, value)]
			if (variant) merge(fieldPaths(root, variant.schema, value, path))
			break
		}
		case 'object':
			for (const property of propertiesOf(node.schema)) {
				const child =
					typeof value === 'object' && value !== null && !Array.isArray(value)
						? value[property.key]
						: undefined
				const childPath = [...path, property.key]
				if (child === undefined || child === null) {
					found.add(formatPath(childPath))
				} else {
					merge(fieldPaths(root, property.schema, child, childPath))
				}
			}
			break
		default:
			break
	}
	return found
}
