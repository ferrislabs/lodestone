import { describe, expect, it } from 'vitest'

import {
	declaredChoiceNote,
	formatLlmScore,
	llmScoreWidth,
	PENDING_SCORING_LABEL,
	scoringProviderText,
	scoringWeightLabel,
	verdictProbabilityRows,
} from '#/pages/leads/lib/scoring'
import type { ScoringResult, ScoringVerdict } from '#/pages/leads/lib/types'

describe('formatLlmScore', () => {
	it('shows the score out of 100, or a dash while pending', () => {
		expect(formatLlmScore(72)).toBe('72 / 100')
		expect(formatLlmScore(0)).toBe('0 / 100')
		expect(formatLlmScore(null)).toBe('—')
		expect(formatLlmScore(undefined)).toBe('—')
	})

	it('bounds the bar width to the score range', () => {
		expect(llmScoreWidth(72)).toBe(72)
		expect(llmScoreWidth(140)).toBe(100)
		expect(llmScoreWidth(-3)).toBe(0)
		expect(llmScoreWidth(null)).toBe(0)
	})
})

describe('verdictProbabilityRows', () => {
	it('orders the three verdicts with their French labels', () => {
		expect(
			verdictProbabilityRows({ fiable: 30, a_verifier: 60, ecarte: 10 }),
		).toEqual([
			{ key: 'fiable', label: 'Fiable', percent: 30 },
			{ key: 'a_verifier', label: 'À vérifier', percent: 60 },
			{ key: 'ecarte', label: 'Écarté', percent: 10 },
		])
		expect(verdictProbabilityRows(null)).toEqual([])
	})
})

describe('scoring labels', () => {
	const verdict: ScoringVerdict = {
		choice: 'a_verifier',
		declared_choice: null,
		confidence: 60,
		probabilities: { fiable: 30, a_verifier: 60, ecarte: 10 },
	}

	it('names the pending verdict after the scoring', () => {
		expect(PENDING_SCORING_LABEL).toBe('En attente du scoring')
	})

	it('notes a declared verdict that differs from the probabilities', () => {
		expect(declaredChoiceNote(verdict)).toBeNull()
		expect(declaredChoiceNote({ ...verdict, declared_choice: 'fiable' })).toBe(
			'Verdict annoncé par le modèle : Fiable, ramené à ses probabilités',
		)
	})

	it('names the provider and the weight of an agent', () => {
		const result = {
			provider_name: 'Synthèse',
			model: 'gemma_laya',
		} as ScoringResult
		expect(scoringProviderText(result)).toBe('Synthèse (gemma_laya)')
		expect(
			scoringProviderText({ provider_name: null, model: '' } as ScoringResult),
		).toBe('Fournisseur inconnu')
		expect(scoringWeightLabel(5)).toBe('poids 5')
		expect(scoringWeightLabel(null)).toBe('')
	})
})
