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
		).toEqual({ max_retries: 'must be a non-negative integer' })
		expect(numericErrors(EMPTY_PROVIDER_VALUES)).toEqual({})
	})
})
