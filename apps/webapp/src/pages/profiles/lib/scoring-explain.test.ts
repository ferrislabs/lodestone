import { describe, expect, it } from 'vitest'

import {
	exampleStrength,
	STRENGTH_ROWS,
	thresholdSummary,
	workedExample,
} from '#/pages/profiles/lib/scoring-explain'

describe('scoring explanation', () => {
	it('computes the worked example from the first two factors', () => {
		expect(
			workedExample([
				{ signal: 'product_detected', weight: 10, value: 'keycloak' },
				{ signal: 'job_offer', weight: 8, value: '' },
				{ signal: 'public_tender', weight: 7, value: '' },
			]),
		).toBe(
			"Un lead avec un signal « Produit détecté · keycloak » (force 0,9) et un signal « Offre d'emploi » (force 0,7) : 10 × 0,9 + 8 × 0,7 = 14,6 points",
		)
	})

	it('skips distinct signals and invalid weights', () => {
		expect(
			workedExample([
				{ signal: 'distinct_signals', weight: 2, value: '' },
				{ signal: 'job_offer', weight: 30, value: '' },
				{ signal: 'debt_marker', weight: 5, value: '' },
			]),
		).toBeNull()
	})

	it('has no example with fewer than two factors', () => {
		expect(workedExample([])).toBeNull()
		expect(
			workedExample([{ signal: 'job_offer', weight: 8, value: '' }]),
		).toBeNull()
	})

	it('uses the strengths found in the sources', () => {
		expect(exampleStrength('product_detected')).toBe(0.9)
		expect(exampleStrength('debt_marker')).toBe(0.6)
		expect(exampleStrength('job_offer')).toBe(0.7)
		expect(
			STRENGTH_ROWS.every((row) => row.strength > 0 && row.strength <= 1),
		).toBe(true)
	})

	it('summarizes the threshold', () => {
		expect(thresholdSummary(40, 18.8)).toBe(
			`Seuil actuel : 40${String.fromCharCode(160)}% = 18,8 points`,
		)
		expect(thresholdSummary(null, null)).toBeNull()
	})
})
