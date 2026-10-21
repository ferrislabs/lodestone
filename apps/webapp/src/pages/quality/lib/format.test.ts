import { describe, expect, it } from 'vitest'

import {
	barHeight,
	bucketLabel,
	discardSuggestion,
	formatRatio,
	formatScore,
	formatSeparation,
	hasDecisions,
	histogramMax,
	insufficientMessage,
	reliableSuggestion,
	versionAgentLabel,
} from './format'
import type { ProfileQuality } from './types'

const profile: ProfileQuality = {
	profile_id: 'p1',
	name: 'Fintech',
	validated: 10,
	rejected: 10,
	precision: 0.5,
	current: { reliable_from: 70, discard_below: 30 },
	suggestion: {
		ready: true,
		labelled: 20,
		required: 20,
		reliable_from: 65,
		discard_below: 25,
	},
	rejection_reasons: [],
}

function plain(text: string): string {
	return text.replace(/\s/g, ' ')
}

describe('formatting', () => {
	it('shows a dash when there is nothing to show', () => {
		expect(formatRatio(null)).toBe('—')
		expect(formatScore(undefined)).toBe('—')
		expect(formatSeparation(null)).toBe('—')
	})

	it('formats ratios, scores and separations in French', () => {
		expect(plain(formatRatio(0.667))).toBe('67 %')
		expect(plain(formatScore(0.85))).toBe('85 %')
		expect(formatSeparation(0.6)).toBe('+60 pts')
		expect(formatSeparation(-0.15)).toBe('-15 pts')
	})

	it('words the suggestions with the current values', () => {
		expect(reliableSuggestion(profile)).toBe(
			'Seuil « Fiable » suggéré : 65 % (actuel 70 %)',
		)
		expect(discardSuggestion(profile)).toBe(
			'Seuil « Écarté » suggéré : 25 % (actuel 30 %)',
		)
		expect(
			reliableSuggestion({
				...profile,
				suggestion: { ...profile.suggestion, reliable_from: null },
			}),
		).toContain('Aucun seuil « Fiable » suggéré')
	})

	it('words the lack of decisions', () => {
		expect(insufficientMessage({ labelled: 7, required: 20 })).toBe(
			'Pas assez de décisions (7/20)',
		)
	})
})

describe('histogram helpers', () => {
	it('labels buckets and scales bars to the tallest', () => {
		expect(bucketLabel({ from: 90, to: 100 })).toBe('90–100 %')
		const buckets = [
			{ validated: 2, rejected: 4 },
			{ validated: 1, rejected: 0 },
		]
		expect(histogramMax(buckets)).toBe(4)
		expect(barHeight(2, 4)).toBe(50)
		expect(barHeight(0, 0)).toBe(0)
	})
})

describe('hasDecisions', () => {
	it('is false only when every count is zero', () => {
		const zero = {
			validated: 0,
			rejected: 0,
			drafts_approved: 0,
			drafts_rejected: 0,
		}
		expect(hasDecisions({ decisions: zero })).toBe(false)
		expect(hasDecisions({ decisions: { ...zero, drafts_rejected: 1 } })).toBe(
			true,
		)
	})
})

describe('versionAgentLabel', () => {
	it('names the aggregator, the agent, or falls back to the id', () => {
		expect(versionAgentLabel({ agent_id: null, agent_name: null })).toBe(
			'Agrégateur',
		)
		expect(
			versionAgentLabel({ agent_id: 'fit', agent_name: 'Adéquation' }),
		).toBe('Adéquation')
		expect(versionAgentLabel({ agent_id: 'fit', agent_name: null })).toBe('fit')
	})
})
