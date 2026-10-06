import { describe, expect, it } from 'vitest'

import {
	confidenceBreakdown,
	evidenceComponentRows,
	verdictReasonLabel,
} from './confidence'

const evidence = {
	strength: 35,
	components: { volume: 28.4, diversity: 50, environment: 0, strength: 24.2 },
	signal_count: 1,
	signal_score: 12.1,
	only_non_production: true,
}

describe('confidenceBreakdown', () => {
	it('sums up the opinion and the evidence strength', () => {
		expect(confidenceBreakdown({ opinion: 100, evidence })).toMatch(
			/^Avis 100\s%\s·\sPreuves 35\s%$/,
		)
	})

	it('is absent until the lead has been scored', () => {
		expect(confidenceBreakdown({ opinion: null, evidence })).toBeNull()
		expect(confidenceBreakdown({ opinion: 80, evidence: null })).toBeNull()
		expect(confidenceBreakdown({})).toBeNull()
	})
})

describe('verdictReasonLabel', () => {
	it('explains every cap in French', () => {
		expect(verdictReasonLabel('single_angle')).toBe(
			"Plafonné à « À vérifier » : un seul angle d'analyse",
		)
		expect(verdictReasonLabel('weak_evidence')).toBe(
			'Plafonné à « À vérifier » : preuves insuffisantes',
		)
		expect(verdictReasonLabel('non_production')).toBe(
			'Plafonné à « À vérifier » : uniquement des hôtes hors production',
		)
	})
})

describe('evidenceComponentRows', () => {
	it('lists the four components in display order', () => {
		expect(evidenceComponentRows(evidence)).toEqual([
			{ key: 'volume', label: 'Volume de signaux', value: 28.4 },
			{ key: 'diversity', label: 'Diversité des sources', value: 50 },
			{ key: 'environment', label: 'Environnement', value: 0 },
			{ key: 'strength', label: 'Force des signaux', value: 24.2 },
		])
	})
})
