import { translateApiMessage } from '#/lib/api-errors'
import {
	type KeyMode,
	type Provider,
	type ProviderFieldName,
	type ProviderFormValues,
	REASONING_EFFORTS,
	type ReasoningEffort,
} from '#/pages/llm/lib/types'

export const EMPTY_PROVIDER_VALUES: ProviderFormValues = {
	name: '',
	kind: 'anthropic',
	base_url: '',
	model: '',
	max_retries: '',
	timeout_seconds: '',
	input_cents_per_mtok: '',
	output_cents_per_mtok: '',
	api_key: '',
	reasoning_effort: '',
	max_tokens: '',
	temperature: '',
	frequency_penalty: '',
	presence_penalty: '',
	repeat_penalty: '',
}

function text(value: number | null | undefined): string {
	return value === null || value === undefined ? '' : String(value)
}

export function valuesFromProvider(provider: Provider): ProviderFormValues {
	return {
		name: provider.name,
		kind: provider.kind,
		base_url: provider.base_url,
		model: provider.model,
		max_retries: String(provider.max_retries),
		timeout_seconds: String(provider.timeout_seconds),
		input_cents_per_mtok: String(provider.pricing.input_cents_per_mtok),
		output_cents_per_mtok: String(provider.pricing.output_cents_per_mtok),
		api_key: '',
		reasoning_effort: provider.reasoning_effort ?? '',
		max_tokens: text(provider.max_tokens),
		temperature: text(provider.temperature),
		frequency_penalty: text(provider.frequency_penalty),
		presence_penalty: text(provider.presence_penalty),
		repeat_penalty: text(provider.repeat_penalty),
	}
}

const NUMERIC_FIELDS: readonly ProviderFieldName[] = [
	'max_retries',
	'timeout_seconds',
	'input_cents_per_mtok',
	'output_cents_per_mtok',
	'max_tokens',
]

const DECIMAL_FIELDS = [
	'temperature',
	'frequency_penalty',
	'presence_penalty',
	'repeat_penalty',
] as const

type DecimalField = (typeof DECIMAL_FIELDS)[number]

const FIELD_PATHS: Record<ProviderFieldName, string> = {
	name: 'name',
	kind: 'kind',
	base_url: 'base_url',
	model: 'model',
	max_retries: 'max_retries',
	timeout_seconds: 'timeout_seconds',
	input_cents_per_mtok: 'pricing.input_cents_per_mtok',
	output_cents_per_mtok: 'pricing.output_cents_per_mtok',
	api_key: 'api_key',
	reasoning_effort: 'reasoning_effort',
	max_tokens: 'max_tokens',
	temperature: 'temperature',
	frequency_penalty: 'frequency_penalty',
	presence_penalty: 'presence_penalty',
	repeat_penalty: 'repeat_penalty',
}

export function fieldErrorsFromPaths(
	fields: Readonly<Record<string, string>>,
): Partial<Record<ProviderFieldName, string>> {
	const errors: Partial<Record<ProviderFieldName, string>> = {}
	for (const [field, path] of Object.entries(FIELD_PATHS)) {
		const message = fields[path]
		if (message !== undefined) errors[field as ProviderFieldName] = message
	}
	return errors
}

export function numericErrors(
	values: ProviderFormValues,
): Partial<Record<ProviderFieldName, string>> {
	const errors: Partial<Record<ProviderFieldName, string>> = {}
	for (const field of NUMERIC_FIELDS) {
		const raw = values[field].trim()
		if (raw !== '' && !/^\d+$/.test(raw)) {
			errors[field] = translateApiMessage('must be a non-negative integer')
		}
	}
	for (const field of DECIMAL_FIELDS) {
		const raw = values[field].trim()
		if (raw !== '' && !/^-?\d+(?:[.,]\d+)?$/.test(raw)) {
			errors[field] = translateApiMessage('must be a number')
		}
	}
	return errors
}

function optionalNumber(raw: string): number | undefined {
	const trimmed = raw.trim().replace(',', '.')
	return trimmed === '' ? undefined : Number(trimmed)
}

interface GenerationBody {
	max_tokens?: number
	temperature?: number
	frequency_penalty?: number
	presence_penalty?: number
	repeat_penalty?: number
}

interface GenerationUpdate {
	max_tokens: number | null
	temperature: number | null
	frequency_penalty: number | null
	presence_penalty: number | null
	repeat_penalty: number | null
}

function generationBody(values: ProviderFormValues): GenerationBody {
	const body: GenerationBody = {}
	const maxTokens = optionalNumber(values.max_tokens)
	if (maxTokens !== undefined) body.max_tokens = maxTokens
	for (const field of DECIMAL_FIELDS) {
		const value = optionalNumber(values[field])
		if (value !== undefined) body[field] = value
	}
	return body
}

function generationUpdate(values: ProviderFormValues): GenerationUpdate {
	const decimal = (field: DecimalField) => optionalNumber(values[field]) ?? null
	return {
		max_tokens: optionalNumber(values.max_tokens) ?? null,
		temperature: decimal('temperature'),
		frequency_penalty: decimal('frequency_penalty'),
		presence_penalty: decimal('presence_penalty'),
		repeat_penalty: decimal('repeat_penalty'),
	}
}

interface CommonBody {
	name: string
	kind: string
	base_url: string
	model: string
	max_retries?: number
	timeout_seconds?: number
	pricing?: {
		input_cents_per_mtok?: number
		output_cents_per_mtok?: number
	}
}

function commonBody(values: ProviderFormValues): CommonBody {
	const body: CommonBody = {
		name: values.name.trim(),
		kind: values.kind,
		base_url: values.base_url.trim(),
		model: values.model.trim(),
	}
	const retries = optionalNumber(values.max_retries)
	if (retries !== undefined) body.max_retries = retries
	const timeout = optionalNumber(values.timeout_seconds)
	if (timeout !== undefined) body.timeout_seconds = timeout
	const input = optionalNumber(values.input_cents_per_mtok)
	const output = optionalNumber(values.output_cents_per_mtok)
	if (input !== undefined || output !== undefined) {
		body.pricing = {}
		if (input !== undefined) body.pricing.input_cents_per_mtok = input
		if (output !== undefined) body.pricing.output_cents_per_mtok = output
	}
	return body
}

function reasoningOrNull(values: ProviderFormValues): ReasoningEffort | null {
	return (
		REASONING_EFFORTS.find((effort) => effort === values.reasoning_effort) ??
		null
	)
}

export function buildCreateBody(
	values: ProviderFormValues,
): CommonBody &
	GenerationBody & { api_key?: string; reasoning_effort?: ReasoningEffort } {
	const body: CommonBody &
		GenerationBody & {
			api_key?: string
			reasoning_effort?: ReasoningEffort
		} = { ...commonBody(values), ...generationBody(values) }
	const effort = reasoningOrNull(values)
	if (effort !== null) body.reasoning_effort = effort
	if (values.api_key !== '') body.api_key = values.api_key
	return body
}

export function buildUpdateBody(
	values: ProviderFormValues,
	keyMode: KeyMode,
): CommonBody &
	GenerationUpdate & {
		api_key?: string | null
		reasoning_effort: ReasoningEffort | null
	} {
	const body: CommonBody &
		GenerationUpdate & {
			api_key?: string | null
			reasoning_effort: ReasoningEffort | null
		} = {
		...commonBody(values),
		...generationUpdate(values),
		reasoning_effort: reasoningOrNull(values),
	}
	if (keyMode === 'replace') body.api_key = values.api_key
	if (keyMode === 'remove') body.api_key = null
	return body
}
