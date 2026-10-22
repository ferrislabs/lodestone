import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import type { Signal } from '#/pages/leads/lib/types'
import { SignalsList } from './signals-list'

afterEach(cleanup)

const offer: Signal = {
	id: 's1',
	kind: 'job_offer',
	value: 'Ingénieur IAM',
	sentence:
		"Offre d'emploi : Ingénieur IAM (entreprise : Acme ; mots-clés : keycloak)",
	source: 'france_travail',
	agent: 'offres-fr',
	provenance: 'https://candidat.francetravail.fr/offres/1',
	strength: 0.7,
	observed_at: '2026-02-01T00:00:00Z',
	raw: {
		matched: ['keycloak'],
		excerpts: [
			'…nous migrons notre annuaire vers Keycloak pour déployer le SSO…',
			'Expérience de MFA exigée',
		],
	},
}

describe('SignalsList', () => {
	it('shows each excerpt of a signal as a quote under it', () => {
		const { container } = render(<SignalsList signals={[offer]} />)
		const quotes = [...container.querySelectorAll('blockquote')]
		expect(quotes.map((quote) => quote.textContent)).toEqual([
			'…nous migrons notre annuaire vers Keycloak pour déployer le SSO…',
			'Expérience de MFA exigée',
		])
		expect(
			screen.getByText(
				"Offre d'emploi : Ingénieur IAM (entreprise : Acme ; mots-clés : keycloak)",
			),
		).toBeTruthy()
	})

	it('shows no quote for a signal stored without excerpts', () => {
		const { container } = render(
			<SignalsList
				signals={[
					{ ...offer, raw: { matched: ['keycloak'] } },
					{ ...offer, id: 's2', raw: { excerpts: [] } },
				]}
			/>,
		)
		expect(container.querySelector('blockquote')).toBeNull()
	})
})
