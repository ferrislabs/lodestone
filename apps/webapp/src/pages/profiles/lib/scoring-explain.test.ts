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

	it('has no row for the contacts nor for a model triage', () => {
		const text = STRENGTH_ROWS.map((row) => `${row.origin} ${row.signal}`).join(
			' ',
		)
		expect(text).not.toMatch(/contact|security\.txt|adresse|LLM/i)
	})

	it('lists the keyword candidates under the minimum and the certificates', () => {
		const below = STRENGTH_ROWS.find(
			(row) => row.origin === 'Candidat par mots-clés sous le minimum',
		)
		expect(below).toEqual({
			origin: 'Candidat par mots-clés sous le minimum',
			signal: "Appels d'offres, offres d'emploi",
			strength: 0.5,
		})
		const certificate = STRENGTH_ROWS.find(
			(row) => row.signal === 'Certificat observé',
		)
		expect(certificate).toEqual({
			origin: 'Certificate Transparency',
			signal: 'Certificat observé',
			strength: 0.3,
		})
	})
})
