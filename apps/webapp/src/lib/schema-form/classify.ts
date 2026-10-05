import { resolve } from '#/lib/schema-form/resolve'
import type { JsonValue, Schema } from '#/lib/schema-form/types'

export interface Property {
	key: string
	schema: Schema
	required: boolean
	hasDefault: boolean
}

export interface Variant {
	title: string
	discriminator: string
	value: JsonValue
	schema: Schema
}

export type Node =
	| { kind: 'const'; value: JsonValue; schema: Schema }
	| { kind: 'enum'; options: JsonValue[]; schema: Schema }
	| { kind: 'nullable'; inner: Schema; schema: Schema }
	| {
			kind: 'union'
			discriminator: string
			variants: Variant[]
			schema: Schema
	  }
	| { kind: 'object'; properties: Property[]; schema: Schema }
	| { kind: 'array'; items: Schema; schema: Schema }
	| { kind: 'string'; schema: Schema }
	| { kind: 'number'; integer: boolean; schema: Schema }
	| { kind: 'boolean'; schema: Schema }
	| { kind: 'unknown'; schema: Schema }

function isNullSchema(schema: Schema): boolean {
	return schema.type === 'null'
}

function typeNames(schema: Schema): string[] {
	if (schema.type === undefined) return []
	return Array.isArray(schema.type) ? schema.type : [schema.type]
}

function withoutNull(schema: Schema): Schema | null {
	const names = typeNames(schema)
	if (!names.includes('null')) return null
	const rest = names.filter((name) => name !== 'null')
	return { ...schema, type: rest.length === 1 ? rest[0] : rest }
}

function nullableInner(root: Schema, schema: Schema): Schema | null {
	if (schema.anyOf !== undefined) {
		const options = schema.anyOf.map((option) => resolve(root, option))
		const present = options.filter((option) => !isNullSchema(option))
		if (present.length === 1 && options.length === 2) {
			const { anyOf: _anyOf, ...outer } = schema
			return { ...present[0], ...outer }
		}
		return null
	}
	return withoutNull(schema)
}

function discriminatorOf(variant: Schema): [string, JsonValue] | null {
	for (const [key, property] of Object.entries(variant.properties ?? {})) {
		if (property.const !== undefined) return [key, property.const]
	}
	return null
}

function variantsOf(root: Schema, schema: Schema): Variant[] | null {
	if (schema.oneOf === undefined) return null
	const variants: Variant[] = []
	for (const option of schema.oneOf) {
		const resolved = resolve(root, option)
		const discriminator = discriminatorOf(resolved)
		if (discriminator === null) return null
		variants.push({
			title: resolved.title ?? String(discriminator[1]),
			discriminator: discriminator[0],
			value: discriminator[1],
			schema: resolved,
		})
	}
	const names = new Set(variants.map((variant) => variant.discriminator))
	return variants.length > 0 && names.size === 1 ? variants : null
}

export function propertiesOf(schema: Schema): Property[] {
	const properties = schema.properties ?? {}
	const required = schema.required ?? []
	const ordered = [
		...required.filter((key) => key in properties),
		...Object.keys(properties)
			.filter((key) => !required.includes(key))
			.sort(),
	]
	return ordered.map((key) => ({
		key,
		schema: properties[key] ?? {},
		required: required.includes(key),
		hasDefault: properties[key]?.default !== undefined,
	}))
}

export function classify(root: Schema, input: Schema): Node {
	const schema = resolve(root, input)
	if (schema.const !== undefined) {
		return { kind: 'const', value: schema.const, schema }
	}
	if (schema.enum !== undefined) {
		return { kind: 'enum', options: schema.enum, schema }
	}
	const inner = nullableInner(root, schema)
	if (inner !== null) {
		return { kind: 'nullable', inner: resolve(root, inner), schema }
	}
	const variants = variantsOf(root, schema)
	if (variants !== null) {
		return {
			kind: 'union',
			discriminator: variants[0]?.discriminator ?? '',
			variants,
			schema,
		}
	}
	const names = typeNames(schema)
	if (names.includes('object') || schema.properties !== undefined) {
		return { kind: 'object', properties: propertiesOf(schema), schema }
	}
	if (names.includes('array')) {
		return { kind: 'array', items: schema.items ?? {}, schema }
	}
	if (names.includes('string')) return { kind: 'string', schema }
	if (names.includes('integer')) {
		return { kind: 'number', integer: true, schema }
	}
	if (names.includes('number')) {
		return { kind: 'number', integer: false, schema }
	}
	if (names.includes('boolean')) return { kind: 'boolean', schema }
	return { kind: 'unknown', schema }
}

export function isOptionalProperty(root: Schema, property: Property): boolean {
	if (classify(root, property.schema).kind === 'nullable') return true
	return !property.required && !property.hasDefault
}

export function unwrapNullable(root: Schema, schema: Schema): Schema {
	const node = classify(root, schema)
	return node.kind === 'nullable' ? node.inner : resolve(root, schema)
}

export function variantIndex(
	variants: readonly Variant[],
	value: JsonValue | undefined,
): number {
	if (typeof value !== 'object' || value === null || Array.isArray(value)) {
		return 0
	}
	const discriminator = variants[0]?.discriminator ?? ''
	const index = variants.findIndex(
		(variant) => variant.value === value[discriminator],
	)
	return index === -1 ? 0 : index
}
