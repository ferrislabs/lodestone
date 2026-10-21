import { describe, expect, it } from 'vitest'

import {
	providerKindLabel,
	reasoningLabel,
	testErrorHint,
	testErrorLabel,
	tierLabel,
	tierShortLabel,
} from './labels'

describe('llm labels', () => {
	it('labels the tiers in french', () => {
		expect(tierLabel('triage')).toBe(
			'Classification (agents de qualification par défaut)',
		)
		expect(tierLabel('qualification')).toBe('Synthèse (agrégateur par défaut)')
		expect(tierLabel('other')).toBe('other')
		expect(tierShortLabel('triage')).toBe('Classification')
		expect(tierShortLabel('qualification')).toBe('Synthèse')
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

	it('labels the reasoning exhaustion with a hint', () => {
		expect(testErrorLabel('reasoning_exhausted')).toBe(
			'Le modèle a consommé tous ses tokens à raisonner',
		)
		expect(testErrorHint('reasoning_exhausted')).toContain('Désactivé')
		expect(testErrorHint('timeout')).toBeNull()
	})

	it('labels a model that rejects the reasoning setting', () => {
		expect(testErrorLabel('reasoning_unsupported')).toBe(
			'Raisonnement non pris en charge',
		)
	})

	it('labels the reasoning levels', () => {
		expect(reasoningLabel('none')).toBe('Désactivé')
		expect(reasoningLabel('high')).toBe('Élevé')
	})
})
