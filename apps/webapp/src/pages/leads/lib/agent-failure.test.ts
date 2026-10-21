import { describe, expect, it } from 'vitest'

import {
	awaitingFailureLabel,
	failureOfKind,
	retryStatusLabel,
} from '#/pages/leads/lib/agent-failure'
import type { AgentFailure } from '#/pages/leads/lib/types'

const failure: AgentFailure = {
	kind: 'scoring',
	agent_id: 'a',
	agent_name: 'A',
	attempts: 1,
	failed: false,
	error: 'x',
	next_attempt_at: '2026-02-01T00:01:00Z',
}

describe('agent failure wording', () => {
	it('announces the next attempt while the backoff runs', () => {
		expect(retryStatusLabel(failure)).toMatch(/^Nouvelle tentative : /)
		expect(retryStatusLabel({ ...failure, next_attempt_at: null })).toBe(
			'Nouvelle tentative au prochain passage',
		)
	})

	it('reports the failure after the attempts', () => {
		const failed = { ...failure, attempts: 5, failed: true }
		expect(retryStatusLabel(failed)).toBe('En échec après 5 tentatives')
		expect(awaitingFailureLabel(failed)).toBe(
			'En attente : appel au modèle en échec après 5 tentatives',
		)
	})

	it('explains the pending result with the next attempt', () => {
		expect(awaitingFailureLabel(failure)).toMatch(
			/^En attente : appel au modèle en échec, nouvelle tentative /,
		)
	})

	it('picks the failure of one kind', () => {
		const qualification = { ...failure, kind: 'qualification' as const }
		expect(failureOfKind([qualification, failure], 'scoring')).toBe(failure)
		expect(failureOfKind([qualification], 'scoring')).toBeNull()
	})
})
