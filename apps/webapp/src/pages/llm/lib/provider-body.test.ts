import { describe, expect, it } from 'vitest'

import {
	buildCreateBody,
	buildUpdateBody,
	EMPTY_PROVIDER_VALUES,
	fieldErrorsFromPaths,
	numericErrors,
	valuesFromProvider,
} from './provider-body'
import type { Provider } from './types'

const provider: Provider = {
	id: 'p1',
	name: 'Claude',
	kind: 'anthropic',
	base_url: 'https://api.anthropic.com',
	model: 'claude-x',
	enabled: true,
	has_api_key: true,
	max_retries: 2,
	timeout_seconds: 30,
	pricing: { input_cents_per_mtok: 300, output_cents_per_mtok: 1500 },
	reasoning_effort: null,
	max_tokens: null,
	temperature: null,
	frequency_penalty: null,
	presence_penalty: null,
	repeat_penalty: null,
	tiers: ['triage'],
	created_at: '2026-01-01T00:00:00Z',
	updated_at: '2026-01-01T00:00:00Z',
}

describe('valuesFromProvider', () => {
	it('never carries a key, even when the provider has one', () => {
		expect(valuesFromProvider(provider).api_key).toBe('')
	})
})

describe('buildUpdateBody', () => {
	const values = { ...valuesFromProvider(provider), api_key: 'sk-new' }

	it('omits api_key when keeping the current key', () => {
		const body = buildUpdateBody(values, 'keep')
		expect('api_key' in body).toBe(false)
		expect(JSON.stringify(body)).not.toContain('sk-new')
	})

	it('sends the string when replacing the key', () => {
		expect(buildUpdateBody(values, 'replace').api_key).toBe('sk-new')
	})

	it('sends null when removing the key', () => {
		const body = buildUpdateBody(values, 'remove')
		expect(body.api_key).toBeNull()
		expect(JSON.stringify(body)).toContain('"api_key":null')
		expect(JSON.stringify(body)).not.toContain('sk-new')
	})

	it('sends the pricing object with numbers', () => {
		expect(buildUpdateBody(values, 'keep').pricing).toEqual({
			input_cents_per_mtok: 300,
			output_cents_per_mtok: 1500,
		})
	})
})

describe('reasoning effort', () => {
	it('loads the stored value and defaults to empty', () => {
		expect(valuesFromProvider(provider).reasoning_effort).toBe('')
		expect(
			valuesFromProvider({ ...provider, reasoning_effort: 'none' })
				.reasoning_effort,
		).toBe('none')
	})

	it('sends the value on update and null when cleared', () => {
		const values = valuesFromProvider(provider)
		expect(
			buildUpdateBody({ ...values, reasoning_effort: 'high' }, 'keep')
				.reasoning_effort,
		).toBe('high')
		expect(buildUpdateBody(values, 'keep').reasoning_effort).toBeNull()
		expect(JSON.stringify(buildUpdateBody(values, 'keep'))).toContain(
			'"reasoning_effort":null',
		)
	})

	it('omits it on create unless chosen', () => {
		expect('reasoning_effort' in buildCreateBody(EMPTY_PROVIDER_VALUES)).toBe(
			false,
		)
		expect(
			buildCreateBody({ ...EMPTY_PROVIDER_VALUES, reasoning_effort: 'none' })
				.reasoning_effort,
		).toBe('none')
	})

	it('maps the server path to the field', () => {
		expect(
			fieldErrorsFromPaths({ reasoning_effort: 'must be one of none' }),
		).toEqual({ reasoning_effort: 'must be one of none' })
	})
})

describe('generation parameters', () => {
	const tuned: Provider = {
		...provider,
		max_tokens: 900,
		temperature: 0.2,
		frequency_penalty: 0.4,
		presence_penalty: -0.5,
		repeat_penalty: 1.3,
	}

	it('loads stored values as text and absent ones as empty', () => {
		const values = valuesFromProvider(tuned)
		expect(values.max_tokens).toBe('900')
		expect(values.temperature).toBe('0.2')
		expect(values.presence_penalty).toBe('-0.5')
		expect(values.repeat_penalty).toBe('1.3')
		expect(valuesFromProvider(provider).max_tokens).toBe('')
		expect(valuesFromProvider(provider).temperature).toBe('')
	})

	it('omits blank values on create', () => {
		const body = buildCreateBody(EMPTY_PROVIDER_VALUES)
		for (const field of [
			'max_tokens',
			'temperature',
			'frequency_penalty',
			'presence_penalty',
			'repeat_penalty',
		]) {
			expect(field in body).toBe(false)
		}
	})

	it('sends numbers on create, accepting a decimal comma', () => {
		const body = buildCreateBody({
			...EMPTY_PROVIDER_VALUES,
			max_tokens: '900',
			temperature: '0,2',
			repeat_penalty: '1.3',
		})
		expect(body.max_tokens).toBe(900)
		expect(body.temperature).toBe(0.2)
		expect(body.repeat_penalty).toBe(1.3)
		expect('presence_penalty' in body).toBe(false)
	})

	it('sends the values on update and null for cleared ones', () => {
		const kept = buildUpdateBody(valuesFromProvider(tuned), 'keep')
		expect(kept.max_tokens).toBe(900)
		expect(kept.presence_penalty).toBe(-0.5)
		const cleared = buildUpdateBody(
			{ ...valuesFromProvider(tuned), max_tokens: '', temperature: '0' },
			'keep',
		)
		expect(cleared.max_tokens).toBeNull()
		expect(cleared.temperature).toBe(0)
		expect(JSON.stringify(cleared)).toContain('"max_tokens":null')
	})

	it('maps the server paths to the fields', () => {
		expect(
			fieldErrorsFromPaths({
				temperature: 'must be between 0 and 2',
				repeat_penalty: 'must be between 1 and 2',
			}),
		).toEqual({
			temperature: 'must be between 0 and 2',
			repeat_penalty: 'must be between 1 and 2',
		})
	})

	it('flags non numeric input in French', () => {
		expect(
			numericErrors({
				...EMPTY_PROVIDER_VALUES,
				max_tokens: '1.5',
				temperature: 'chaud',
				frequency_penalty: '-0,5',
			}),
		).toEqual({
			max_tokens: 'Doit être un entier positif ou nul',
			temperature: 'Doit être un nombre',
		})
	})
})

describe('buildCreateBody', () => {
	it('omits blank optional fields and an empty key', () => {
		const body = buildCreateBody({
			...EMPTY_PROVIDER_VALUES,
			name: ' Claude ',
			base_url: 'https://x.test',
			model: 'm',
		})
		expect(body).toEqual({
			name: 'Claude',
			kind: 'anthropic',
			base_url: 'https://x.test',
			model: 'm',
		})
	})

	it('includes the key when typed', () => {
		const body = buildCreateBody({ ...EMPTY_PROVIDER_VALUES, api_key: 'k' })
		expect(body.api_key).toBe('k')
	})
})

describe('field mapping', () => {
	it('maps server paths to form fields', () => {
		expect(
			fieldErrorsFromPaths({
				name: 'must not be empty',
				'pricing.input_cents_per_mtok': 'too big',
				api_key: 'must not be empty',
			}),
		).toEqual({
			name: 'must not be empty',
			input_cents_per_mtok: 'too big',
			api_key: 'must not be empty',
		})
	})

	it('flags non numeric input', () => {
		expect(
			numericErrors({ ...EMPTY_PROVIDER_VALUES, max_retries: '2x' }),
		).toEqual({ max_retries: 'Doit être un entier positif ou nul' })
		expect(numericErrors(EMPTY_PROVIDER_VALUES)).toEqual({})
	})
})
