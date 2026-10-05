import { describe, expect, it } from 'vitest'

import { defaultValue, toJsonObject } from '#/lib/schema-form'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import { prepareSave } from '#/pages/profiles/lib/save'

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
			schema_version: 1,
			identity: { offer: 'o', target_customer: 't' },
			sources: {},
			scoring: { factors: [] },
			contact_ranking: { preferred_local_parts: [] },
			llm: { qualification_threshold: 50, relevance_criteria: 'r' },
		}
		expect(prepareSave(filled).ok).toBe(true)
	})
})
