export type JsonValue =
	| string
	| number
	| boolean
	| null
	| JsonValue[]
	| { [key: string]: JsonValue }

export type JsonObject = { [key: string]: JsonValue }

export type Segment = string | number

export interface Schema {
	$ref?: string
	$defs?: Record<string, Schema>
	title?: string
	description?: string
	type?: string | string[]
	format?: string
	properties?: Record<string, Schema>
	required?: string[]
	items?: Schema
	anyOf?: Schema[]
	oneOf?: Schema[]
	const?: JsonValue
	enum?: JsonValue[]
	default?: JsonValue
	minimum?: number
	maximum?: number
	minLength?: number
	maxLength?: number
}

export interface Problem {
	path: string
	message: string
}

export interface UiHints {
	hidden: readonly string[]
	textarea: readonly string[]
	defaults: Readonly<Record<string, JsonValue>>
	labels: Readonly<Record<string, string>>
}

export const NO_HINTS: UiHints = {
	hidden: [],
	textarea: [],
	defaults: {},
	labels: {},
}

export function isJsonObject(value: unknown): value is JsonObject {
	return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function toJsonValue(value: unknown): JsonValue {
	if (
		typeof value === 'string' ||
		typeof value === 'number' ||
		typeof value === 'boolean'
	) {
		return value
	}
	if (Array.isArray(value)) return value.map(toJsonValue)
	if (typeof value === 'object' && value !== null) {
		const entries: [string, JsonValue][] = []
		for (const [key, item] of Object.entries(value)) {
			if (item !== undefined) entries.push([key, toJsonValue(item)])
		}
		return Object.fromEntries(entries)
	}
	return null
}

export function toJsonObject(value: unknown): JsonObject {
	const converted = toJsonValue(value)
	return isJsonObject(converted) ? converted : {}
}

export function toSchema(value: unknown): Schema {
	return isJsonObject(value) ? (value as Schema) : {}
}
