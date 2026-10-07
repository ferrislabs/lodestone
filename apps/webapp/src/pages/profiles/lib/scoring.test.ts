import { describe, expect, it } from 'vitest'
import { isValidThreshold, readThreshold } from '#/pages/profiles/lib/scoring'

describe('scoring helpers', () => {
	it('validates thresholds', () => {
		expect([0, 100, 101, -1, 3.5, null].map(isValidThreshold)).toEqual([
			true,
			true,
			false,
			false,
			false,
			false,
		])
	})

	it('reads the threshold from a definition', () => {
		expect(readThreshold({ llm: { qualification_threshold: 40 } })).toBe(40)
		expect(readThreshold({ llm: {} })).toBeNull()
		expect(readThreshold({})).toBeNull()
	})
})
