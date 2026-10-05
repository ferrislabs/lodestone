import { translateApiMessage } from '#/lib/api-errors'
import type {
	KeyMode,
	Provider,
	ProviderFieldName,
	ProviderFormValues,
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
	}
}

const NUMERIC_FIELDS: readonly ProviderFieldName[] = [
	'max_retries',
	'timeout_seconds',
	'input_cents_per_mtok',
	'output_cents_per_mtok',
]

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
	return errors
}

function optionalNumber(raw: string): number | undefined {
	const trimmed = raw.trim()
	return trimmed === '' ? undefined : Number(trimmed)
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

export function buildCreateBody(
	values: ProviderFormValues,
): CommonBody & { api_key?: string } {
	const body: CommonBody & { api_key?: string } = commonBody(values)
	if (values.api_key !== '') body.api_key = values.api_key
	return body
}

export function buildUpdateBody(
	values: ProviderFormValues,
	keyMode: KeyMode,
): CommonBody & { api_key?: string | null } {
	const body: CommonBody & { api_key?: string | null } = commonBody(values)
	if (keyMode === 'replace') body.api_key = values.api_key
	if (keyMode === 'remove') body.api_key = null
	return body
}
