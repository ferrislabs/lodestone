import { describe, expect, it } from 'vitest'

import {
	formatAgentScore,
	formatConfidence,
	formatProbability,
	probabilityWidth,
	verdictKey,
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
