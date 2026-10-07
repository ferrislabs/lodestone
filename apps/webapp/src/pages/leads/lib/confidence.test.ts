import { describe, expect, it } from 'vitest'

import {
	confidenceBreakdown,
	evidenceComponentRows,
	exclusionSummary,
	exclusionTitle,
	indicatorCapLabel,
	verdictReasonLabel,
} from './confidence'

const evidence = {
	strength: 35,
	components: { volume: 28.4, diversity: 50, environment: 0, strength: 24.2 },
	signal_count: 1,
	independent_signals: 1,
	decayed_signals: 0.5,
	source_count: 1,
	kind_count: 1,
	signal_score: 12.1,
	only_non_production: true,
}

describe('confidenceBreakdown', () => {
	it('sums up the opinion and the evidence strength', () => {
		expect(confidenceBreakdown({ opinion: 100, evidence })).toMatch(
			/^Indicateurs : avis 100\s%\s×\spreuves 35\s%$/,
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
			{
				key: 'volume',
				label: 'Volume de signaux indépendants',
				value: 28.4,
			},
			{
				key: 'diversity',
				label: 'Diversité des sources et des types',
				value: 50,
			},
			{ key: 'environment', label: 'Environnement', value: 0 },
			{ key: 'strength', label: 'Force des signaux', value: 24.2 },
		])
	})
})

const exclusion = {
	agent_id: 'exclusions',
	agent_name: 'Exclusions',
	question_id: 'concurrent',
	option: 'oui',
	label: 'Concurrent',
	probability: 85,
	evidence: 'Nous sommes un concurrent.',
	evidence_source: 'lead',
}

describe('exclusions', () => {
	it('titles an exclusion with the agent, the option and the probability', () => {
		expect(exclusionTitle(exclusion)).toMatch(
			/^Exclu par l'agent « Exclusions » : Concurrent \(85\s%\)$/,
		)
	})

	it('summarises the exclusions of a lead for the list', () => {
		expect(exclusionSummary([])).toBeNull()
		expect(exclusionSummary([exclusion])).toBe('Exclu : Concurrent')
		expect(
			exclusionSummary([exclusion, { ...exclusion, label: 'Client actuel' }]),
		).toBe('Exclu : Concurrent (+1)')
	})

	it('labels the excluded verdict reason', () => {
		expect(verdictReasonLabel('excluded')).toBe(
			'Écarté : un agent de qualification a exclu le lead',
		)
	})
})

describe('indicator labels', () => {
	it('words the cap of the indicators apart from the verdict', () => {
		expect(indicatorCapLabel('single_angle')).toBe(
			"Indicateurs plafonnés : un seul angle d'analyse",
		)
		expect(indicatorCapLabel('weak_evidence')).toBe(
			'Indicateurs plafonnés : preuves insuffisantes',
		)
	})

	it('words the pending scoring and the exclusion', () => {
		expect(verdictReasonLabel('awaiting_scoring')).toBe(
			"En attente : aucun agent de scoring n'a répondu",
		)
		expect(verdictReasonLabel('excluded')).toBe(
			'Écarté : un agent de qualification a exclu le lead',
		)
	})
})
