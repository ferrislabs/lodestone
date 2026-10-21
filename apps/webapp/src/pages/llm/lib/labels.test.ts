import { describe, expect, it } from 'vitest'

import { providerKindLabel, testErrorLabel, tierLabel } from './labels'

describe('llm labels', () => {
	it('labels the tiers in french', () => {
		expect(tierLabel('triage')).toBe('Tri')
		expect(tierLabel('qualification')).toBe('Qualification')
		expect(tierLabel('other')).toBe('other')
	})

	it('keeps provider product names', () => {
		expect(providerKindLabel('anthropic')).toBe('Anthropic')
		expect(providerKindLabel('mistral')).toBe('Mistral')
		expect(providerKindLabel('openai_compatible')).toBe('Compatible OpenAI')
	})

	it('labels test failure kinds', () => {
		expect(testErrorLabel('timeout')).toBe('Délai dépassé')
		expect(testErrorLabel('unreachable')).toBe('Injoignable')
		expect(testErrorLabel('weird')).toBe('weird')
	})
})
