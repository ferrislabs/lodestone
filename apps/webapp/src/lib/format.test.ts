import { describe, expect, it } from 'vitest'

import { formatRelative } from './format'

const NOW = Date.parse('2026-10-05T12:00:00Z')

describe('formatRelative', () => {
	it('says now for a moment around the reference', () => {
		expect(formatRelative('2026-10-05T12:00:05Z', NOW)).toBe('maintenant')
		expect(formatRelative('2026-10-05T11:59:55Z', NOW)).toBe('maintenant')
	})

	it('speaks of the future and the past in french', () => {
		expect(formatRelative('2026-10-05T12:05:00Z', NOW)).toBe('dans 5 minutes')
		expect(formatRelative('2026-10-05T10:00:00Z', NOW)).toBe('il y a 2 heures')
		expect(formatRelative('2026-10-08T12:00:00Z', NOW)).toBe('dans 3 jours')
		expect(formatRelative('2026-10-05T11:59:30Z', NOW)).toBe(
			'il y a 30 secondes',
		)
	})

	it('keeps an unparsable value', () => {
		expect(formatRelative('soon', NOW)).toBe('soon')
	})
})
