import { describe, expect, it } from 'vitest'

import { qualityWarningLabel, qualityWarningMessage } from './quality'

describe('qualityWarningMessage', () => {
	it('is absent without warnings, including from an older API', () => {
		expect(qualityWarningMessage([])).toBeNull()
		expect(qualityWarningMessage(undefined)).toBeNull()
	})

	it('lists every warning in French', () => {
		expect(qualityWarningMessage(['repetition'])).toBe(
			'Synthèse de qualité incertaine : répétitions',
		)
		expect(
			qualityWarningMessage(['repetition', 'too_long', 'restated_figures']),
		).toBe(
			'Synthèse de qualité incertaine : répétitions, texte trop long, pourcentage ou verdict répétés avec un chiffre',
		)
	})

	it('names the dropped quotes and keeps an unknown warning readable', () => {
		expect(qualityWarningLabel('dropped_quotes')).toBe(
			'citations non retrouvées écartées',
		)
		expect(qualityWarningLabel('nouveau')).toBe('nouveau')
	})
})
