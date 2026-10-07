import { describe, expect, it } from 'vitest'

import { defaultValue, toJsonObject } from '#/lib/schema-form'
import { newAgent } from '#/pages/profiles/lib/agents'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import { newQualificationAgent } from '#/pages/profiles/lib/qualification'
import { prepareSave } from '#/pages/profiles/lib/save'
import { newScoringAgent } from '#/pages/profiles/lib/scoring-agents'

describe('prepareSave', () => {
	it('blocks an empty new profile and lists the fields', () => {
		const blank = toJsonObject(
			defaultValue(PROFILE_SCHEMA, PROFILE_SCHEMA, PROFILE_HINTS),
		)
		const attempt = prepareSave(blank)
		expect(attempt.ok).toBe(false)
		if (!attempt.ok) {
			expect(attempt.problems.map((p) => p.path)).toContain('identity.offer')
		}
	})

	it('accepts a filled definition', () => {
		const filled = {
			schema_version: 6,
			identity: { offer: 'o', target_customer: 't' },
			agents: [newAgent('ats', [])],
			qualification_agents: [newQualificationAgent('pertinence', new Set())],
			aggregator: { provider_id: null, system_prompt: 'p' },
			verdict: { reliable_from: 70, discard_below: 30 },
			contact_ranking: { preferred_local_parts: [] },
			llm: { qualification_threshold: 50, relevance_criteria: 'r' },
		}
		expect(prepareSave(filled).ok).toBe(true)
	})

	it('blocks a scoring agent that breaks the rules the server enforces', () => {
		const filled = {
			schema_version: 13,
			identity: { offer: 'o', target_customer: 't' },
			agents: [newAgent('ats', [])],
			qualification_agents: [newQualificationAgent('pertinence', new Set())],
			scoring_agents: [
				{ ...newScoringAgent(new Set(['ats'])), id: 'pertinence' },
			],
			aggregator: { provider_id: null, system_prompt: 'p' },
			verdict: { reliable_from: 70, discard_below: 30 },
			contact_ranking: { preferred_local_parts: [] },
			llm: { qualification_threshold: 50, relevance_criteria: 'r' },
		}
		const attempt = prepareSave(filled)
		expect(attempt.ok).toBe(false)
		if (!attempt.ok) {
			expect(attempt.problems.map((p) => p.path)).toContain(
				'scoring_agents[0].id',
			)
		}
		expect(
			prepareSave({
				...filled,
				scoring_agents: [newScoringAgent(new Set(['ats', 'pertinence']))],
			}).ok,
		).toBe(true)
	})
})
