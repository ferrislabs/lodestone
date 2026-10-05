import { classify, propertiesOf } from '#/lib/schema-form/classify'
import { patternOf } from '#/lib/schema-form/paths'
import type { Schema, Segment, UiHints } from '#/lib/schema-form/types'

export interface RenderedField {
	pattern: string
	item: boolean
}

function walk(
	root: Schema,
	schema: Schema,
	hints: UiHints,
	path: readonly Segment[],
	found: Map<string, RenderedField>,
	depth: number,
) {
	if (depth > 12) return
	const node = classify(root, schema)
	switch (node.kind) {
		case 'object':
			for (const property of propertiesOf(node.schema)) {
				const child = [...path, property.key]
				const pattern = patternOf(child)
				const inner = classify(root, property.schema)
				if (inner.kind === 'const' || hints.hidden.includes(pattern)) continue
				found.set(pattern, { pattern, item: false })
				walk(root, property.schema, hints, child, found, depth + 1)
			}
			break
		case 'array': {
			const child = [...path, 0]
			const itemNode = classify(root, node.items)
			if (
				itemNode.kind === 'object' ||
				itemNode.kind === 'union' ||
				itemNode.kind === 'array'
			) {
				const pattern = patternOf(child)
				found.set(pattern, { pattern, item: true })
			}
			walk(root, node.items, hints, child, found, depth + 1)
			break
		}
		case 'union':
			for (const variant of node.variants) {
				walk(root, variant.schema, hints, path, found, depth + 1)
			}
			break
		case 'nullable':
			walk(root, node.inner, hints, path, found, depth + 1)
			break
		default:
			break
	}
}

export function renderedFields(root: Schema, hints: UiHints): RenderedField[] {
	const found = new Map<string, RenderedField>()
	walk(root, root, hints, [], found, 0)
	return [...found.values()]
}

export function renderedVariantValues(root: Schema): string[] {
	const values = new Set<string>()
	const visit = (schema: Schema, depth: number) => {
		if (depth > 12) return
		const node = classify(root, schema)
		if (node.kind === 'union') {
			for (const variant of node.variants) {
				values.add(String(variant.value))
				visit(variant.schema, depth + 1)
			}
		} else if (node.kind === 'object') {
			for (const property of propertiesOf(node.schema)) {
				visit(property.schema, depth + 1)
			}
		} else if (node.kind === 'array') {
			visit(node.items, depth + 1)
		} else if (node.kind === 'nullable') {
			visit(node.inner, depth + 1)
		}
	}
	visit(root, 0)
	return [...values]
}
