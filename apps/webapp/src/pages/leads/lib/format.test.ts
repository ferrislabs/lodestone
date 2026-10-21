import { describe, expect, it } from 'vitest'
import {
	externalHref,
	formatDateTime,
	formatRatio,
	formatScore,
	scoreRatio,
} from './format'

const NBSP = String.fromCharCode(160)

describe('format', () => {
	it('formats scores with a decimal comma', () => {
		expect(formatScore(12)).toBe('12')
		expect(formatScore(12.345)).toBe('12,3')
		expect(formatScore(0)).toBe('0')
	})

	it('formats ratios as french percentages', () => {
		expect(formatRatio(0.856)).toBe(`86${NBSP}%`)
		expect(formatRatio(1)).toBe(`100${NBSP}%`)
	})

	it('formats dates in the fr-FR locale', () => {
		expect(formatDateTime('2026-10-05T13:46:00Z')).toMatch(
			/^5 oct\. 2026,? (à )?\d{2}:46$/,
		)
		expect(formatDateTime('not a date')).toBe('not a date')
	})

	it('clamps the score bar ratio', () => {
		expect(scoreRatio(5, 10)).toBe(0.5)
		expect(scoreRatio(20, 10)).toBe(1)
		expect(scoreRatio(-1, 10)).toBe(0)
		expect(scoreRatio(5, 0)).toBe(0)
	})

	it('only links http and https provenance', () => {
		expect(externalHref('https://acme.test/jobs')).toBe(
			'https://acme.test/jobs',
		)
		expect(externalHref('javascript:alert(1)')).toBeNull()
		expect(externalHref('dns:acme.test')).toBeNull()
		expect(externalHref('not a url')).toBeNull()
	})
})
