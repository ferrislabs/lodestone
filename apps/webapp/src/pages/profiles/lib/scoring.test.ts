import { describe, expect, it } from 'vitest'
import {
	factorTitle,
	isAvailable,
	nextSignal,
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
})
