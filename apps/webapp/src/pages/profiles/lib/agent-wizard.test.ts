import { describe, expect, it } from 'vitest'

import {
	nextStep,
	previousStep,
	stepProblems,
	WIZARD_STEPS,
} from '#/pages/profiles/lib/agent-wizard'
import { newAgent } from '#/pages/profiles/lib/agents'

describe('agent wizard steps', () => {
	it('walks the three steps in order', () => {
		expect(WIZARD_STEPS.map((step) => step.label)).toEqual([
			'Type et fournisseurs',
			'Critères',
			'Récapitulatif',
		])
		expect(nextStep('type')).toBe('criteria')
		expect(nextStep('criteria')).toBe('summary')
		expect(nextStep('summary')).toBe('summary')
		expect(previousStep('summary')).toBe('criteria')
		expect(previousStep('type')).toBe('type')
	})

	it('requires a provider then a keyword for a search agent', () => {
		const empty = newAgent('search', [])
		expect(stepProblems('type', empty)).toEqual([
			'Choisir au moins un fournisseur.',
		])
		const withProvider = { ...empty, providers: ['boamp'] }
		expect(stepProblems('type', withProvider)).toEqual([])
		expect(stepProblems('criteria', withProvider)).toEqual([
			'Ajouter au moins un mot-clé.',
		])
		const ready = { ...withProvider, keywords: ['sso'] }
		expect(stepProblems('criteria', ready)).toEqual([])
		expect(stepProblems('summary', ready)).toEqual([])
		expect(stepProblems('summary', { ...ready, name: ' ' })).toEqual([
			"Donner un nom à l'agent.",
		])
	})

	it('requires a label for a certificates agent', () => {
		const empty = { ...newAgent('certificates', []), labels: [] }
		expect(stepProblems('type', empty)).toEqual([])
		expect(stepProblems('criteria', empty)).toEqual([
			'Ajouter au moins un libellé.',
		])
		expect(stepProblems('summary', { ...empty, labels: ['auth'] })).toEqual([])
	})

	it('requires a complete pattern for a dns agent', () => {
		const empty = newAgent('dns', [])
		expect(stepProblems('type', empty)).toEqual([])
		expect(stepProblems('criteria', empty)).toEqual([
			'Ajouter au moins un motif CNAME.',
		])
		expect(
			stepProblems('criteria', {
				...empty,
				patterns: [{ provider: 'Okta', target_suffix: '' }],
			}),
		).toEqual(['Ajouter au moins un motif CNAME.'])
		expect(
			stepProblems('summary', {
				...empty,
				patterns: [{ provider: 'Okta', target_suffix: 'okta.com' }],
			}),
		).toEqual([])
	})
})
