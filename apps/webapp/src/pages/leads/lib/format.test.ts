import { describe, expect, it } from 'vitest'
import { externalHref, formatRatio, formatScore, scoreRatio } from './format'

describe('format', () => {
	it('formats scores', () => {
		expect(formatScore(12)).toBe('12')
		expect(formatScore(12.345)).toBe('12.3')
		expect(formatScore(0)).toBe('0')
	})

	it('formats ratios as percentages', () => {
		expect(formatRatio(0.856)).toBe('86%')
		expect(formatRatio(1)).toBe('100%')
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
