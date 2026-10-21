import type { Schema } from '#/lib/schema-form/types'

const REF_PREFIX = '#/$defs/'

export function resolveRef(root: Schema, ref: string): Schema | null {
	if (!ref.startsWith(REF_PREFIX)) return null
	return root.$defs?.[ref.slice(REF_PREFIX.length)] ?? null
}

export function resolve(root: Schema, node: Schema, depth = 0): Schema {
	if (node.$ref === undefined || depth > 16) return node
	const target = resolveRef(root, node.$ref)
	if (target === null) return node
	const { $ref: _ref, ...siblings } = node
	return resolve(root, { ...target, ...siblings }, depth + 1)
}
