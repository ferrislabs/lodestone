import { describe, expect, it } from 'vitest'

import { STRENGTH_ROWS } from '#/pages/profiles/lib/scoring-explain'

describe('signal strength by origin', () => {
	it('lists strengths between 0 and 1 for every origin', () => {
		expect(STRENGTH_ROWS.length).toBeGreaterThan(0)
		expect(
			STRENGTH_ROWS.every((row) => row.strength > 0 && row.strength <= 1),
		).toBe(true)
	})

	it('gives the strongest strength to a detected product', () => {
		const product = STRENGTH_ROWS.find(
			(row) => row.signal === 'Produit détecté',
		)
		expect(product?.strength).toBe(0.9)
	})
})
