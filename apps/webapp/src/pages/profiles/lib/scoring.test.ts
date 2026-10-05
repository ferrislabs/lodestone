import { describe, expect, it } from 'vitest'
import {
	factorTitle,
	formatPoints,
	isAvailable,
	isValidThreshold,
	isValidWeight,
	maxScore,
	nextSignal,
	qualificationPoints,
	readThreshold,
	SIGNAL_KINDS,
	valueSuggestions,
} from '#/pages/profiles/lib/scoring'
import { IAM } from '#/pages/profiles/lib/scoring-fixture'

describe('scoring helpers', () => {
	it('lists every kind with a label', () => {
		expect(SIGNAL_KINDS.map((meta) => meta.kind)).toContain('distinct_signals')
		expect(SIGNAL_KINDS.every((meta) => meta.label !== '')).toBe(true)
	})

	it('collects sorted unique suggestions, disabled agents included', () => {
		expect(valueSuggestions(IAM, 'product_detected')).toEqual([
			'auth0',
			'keycloak',
		])
		expect(valueSuggestions(IAM, 'debt_marker')).toEqual([
			'basic_auth',
			'legacy_auth_path',
		])
		expect(valueSuggestions(IAM, 'saas_provider')).toEqual(['okta'])
		expect(valueSuggestions(IAM, 'job_offer')).toEqual([])
	})

	it('titles a factor', () => {
		expect(factorTitle({ signal: 'product_detected', value: 'keycloak' })).toBe(
			'Product detected · keycloak',
		)
		expect(factorTitle({ signal: 'job_offer', value: '' })).toBe('Job offer')
	})

	it('knows which signals an agent can emit', () => {
		expect(isAvailable(IAM, 'product_detected')).toBe(true)
		expect(isAvailable(IAM, 'job_offer')).toBe(false)
		expect(isAvailable({ agents: [] }, 'role_contact_found')).toBe(true)
		expect(isAvailable({ agents: [] }, 'distinct_signals')).toBe(true)
	})

	it('picks the next unused available signal', () => {
		expect(
			nextSignal(IAM, [{ signal: 'product_detected', weight: 1, value: '' }]),
		).toBe('debt_marker')
		expect(nextSignal({ agents: [] }, [])).toBe('role_contact_found')
	})

	it('computes the maximum score like the server', () => {
		const factors = [
			{ signal: 'product_detected', weight: 10 },
			{ signal: 'debt_marker', weight: 5 },
			{ signal: 'saas_provider', weight: 3 },
			{ signal: 'job_offer', weight: 8 },
			{ signal: 'public_tender', weight: 7 },
			{ signal: 'role_contact_found', weight: 2 },
			{ signal: 'distinct_signals', weight: 2 },
		]
		expect(maxScore(factors)).toBe(47)
		expect(maxScore([])).toBe(0)
		expect(
			maxScore([
				{ signal: 'product_detected', weight: 4 },
				{ signal: 'product_detected', weight: 4 },
				{ signal: 'distinct_signals', weight: 3 },
			]),
		).toBe(11)
		expect(maxScore([{ signal: 'a', weight: null }])).toBe(0)
	})

	it('turns the threshold percentage into points', () => {
		expect(qualificationPoints(40, 47)).toBeCloseTo(18.8)
		expect(qualificationPoints(100, 47)).toBe(47)
		expect(qualificationPoints(50, 0)).toBe(0)
		expect(formatPoints(18.8)).toBe('18.8')
		expect(formatPoints(24.5)).toBe('24.5')
		expect(formatPoints(18)).toBe('18')
		expect(formatPoints(19.6000001)).toBe('19.6')
	})

	it('validates weights and thresholds', () => {
		expect([0, 1, 10, 11, 2.5, null].map(isValidWeight)).toEqual([
			false,
			true,
			true,
			false,
			false,
			false,
		])
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
