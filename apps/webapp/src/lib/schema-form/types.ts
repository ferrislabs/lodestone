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

export interface FieldCopy {
	label: string
	description?: string
}

export interface UiMessages {
	required: string
	add: string
	remove: string
	addAria: (label: string) => string
	removeAria: (label: string, position: number) => string
	enableAria: (label: string) => string
	typeAria: (label: string) => string
}

export interface UiHints {
	hidden: readonly string[]
	textarea: readonly string[]
	defaults: Readonly<Record<string, JsonValue>>
	labels: Readonly<Record<string, string>>
	fields: Readonly<Record<string, FieldCopy>>
	variants: Readonly<Record<string, string>>
	messages: UiMessages
}

export const DEFAULT_MESSAGES: UiMessages = {
	required: 'Required',
	add: 'Add',
	remove: 'Remove',
	addAria: (label) => `Add ${label}`,
	removeAria: (label, position) => `Remove ${label} ${position}`,
	enableAria: (label) => `Enable ${label}`,
	typeAria: (label) => `${label} type`,
}

export const NO_HINTS: UiHints = {
	hidden: [],
	textarea: [],
	defaults: {},
	labels: {},
	fields: {},
	variants: {},
	messages: DEFAULT_MESSAGES,
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
