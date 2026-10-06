import { describe, expect, it } from 'vitest'

import {
	confidenceWidth,
	DEFAULT_VERDICTS,
	formatAgentScore,
	formatConfidence,
	formatProbability,
	isDefaultVerdicts,
	orderedVerdicts,
	probabilityWidth,
	summaryExcerpt,
	verdictKey,
	verdictKeyLabel,
	verdictLabel,
} from './verdict'

describe('verdict labels', () => {
	it('names every verdict in French', () => {
		expect(verdictLabel('fiable')).toBe('Fiable')
		expect(verdictLabel('a_verifier')).toBe('À vérifier')
		expect(verdictLabel('ecarte')).toBe('Écarté')
	})

	it('treats a missing verdict as pending', () => {
		expect(verdictLabel(null)).toBe('En attente')
		expect(verdictLabel(undefined)).toBe('En attente')
		expect(verdictKey(null)).toBe('en_attente')
		expect(verdictKey('fiable')).toBe('fiable')
	})
})

describe('number formats', () => {
	it('formats the confidence in percent with a French decimal comma', () => {
		expect(formatConfidence(72.5)).toMatch(/^72,5\s%$/)
		expect(formatConfidence(100)).toMatch(/^100\s%$/)
		expect(formatConfidence(null)).toBe('—')
		expect(formatConfidence(undefined)).toBe('—')
	})

	it('formats the agent score and probabilities as percentages', () => {
		expect(formatAgentScore(0.625)).toMatch(/^62,5\s%$/)
		expect(formatAgentScore(null)).toBe('—')
		expect(formatProbability(0.5)).toMatch(/^50\s%$/)
	})

	it('keeps the probability bar width within the track', () => {
		expect(probabilityWidth(0.255)).toBe(26)
		expect(probabilityWidth(-1)).toBe(0)
		expect(probabilityWidth(3)).toBe(100)
	})
})

describe('summary excerpt', () => {
	it('keeps a short summary and collapses its whitespace', () => {
		expect(summaryExcerpt('Un  résumé\ncourt ')).toBe('Un résumé court')
		expect(summaryExcerpt(null)).toBe('')
		expect(summaryExcerpt(undefined)).toBe('')
	})

	it('cuts a long summary near 140 characters with an ellipsis', () => {
		const excerpt = summaryExcerpt('mot '.repeat(80))
		expect(excerpt.length).toBeLessThanOrEqual(140)
		expect(excerpt.endsWith('…')).toBe(true)
	})
})

describe('confidence bar', () => {
	it('keeps the width within 0 and 100', () => {
		expect(confidenceWidth(72.4)).toBe(72)
		expect(confidenceWidth(-5)).toBe(0)
		expect(confidenceWidth(250)).toBe(100)
		expect(confidenceWidth(null)).toBe(0)
	})
})

describe('verdict filters', () => {
	it('hides discarded leads by default', () => {
		expect(DEFAULT_VERDICTS).not.toContain('ecarte')
		expect(isDefaultVerdicts(['en_attente', 'fiable', 'a_verifier'])).toBe(true)
		expect(isDefaultVerdicts(['fiable'])).toBe(false)
		expect(isDefaultVerdicts(['fiable', 'a_verifier', 'ecarte'])).toBe(false)
	})

	it('orders verdicts like the badges and names each key', () => {
		expect(orderedVerdicts(['en_attente', 'ecarte', 'fiable'])).toEqual([
			'fiable',
			'ecarte',
			'en_attente',
		])
		expect(verdictKeyLabel('en_attente')).toBe('En attente')
	})
})
