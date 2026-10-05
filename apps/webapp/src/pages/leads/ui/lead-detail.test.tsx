import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { LeadDetailData } from '#/pages/leads/lib/types'
import { LeadDetail } from './lead-detail'

afterEach(cleanup)

const detail: LeadDetailData = {
	lead: {
		id: 'l1',
		profile: { id: 'p1', name: 'Fintech' },
		domain: 'acme.test',
		status: 'qualified',
		score: 42.5,
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-01T00:00:00Z',
		updated_at: '2026-02-01T00:00:00Z',
		confidence: 72.5,
		verdict: 'fiable',
		qualified_at: '2026-02-01T00:00:00Z',
	},
	qualification_results: [
		{
			agent_id: 'pertinence',
			agent_name: 'Pertinence',
			provider_id: 'prov-1',
			provider_name: 'Laya',
			model: 'gemma_laya',
			status: 'valid',
			agent_score: 0.625,
			answers: [
				{
					question_id: 'pertinence',
					wording: "Ce lead est-il pertinent pour l'offre ?",
					choice: 'moyenne',
					choice_label: 'Moyenne',
					confidence: 0.8,
					probabilities: [
						{ value: 'forte', label: 'Forte', probability: 0.25 },
						{ value: 'moyenne', label: 'Moyenne', probability: 0.5 },
						{ value: 'faible', label: 'Faible', probability: 0.25 },
					],
				},
			],
			error: null,
			created_at: '2026-02-01T00:00:00Z',
		},
		{
			agent_id: 'maturite',
			agent_name: 'Maturité',
			provider_id: null,
			provider_name: null,
			model: '',
			status: 'invalid',
			agent_score: null,
			answers: [],
			error: 'fournisseur indisponible',
			created_at: '2026-02-01T00:00:00Z',
		},
	],
	score_breakdown: [
		{
			signal: 'hiring_page',
			value: 'careers',
			weight: 3,
			measure: 1,
			contribution: 3,
		},
	],
	signals: [
		{
			id: 's1',
			kind: 'hiring_page',
			value: 'careers',
			source: 'crawler',
			agent: 'ats-fr',
			provenance: 'https://acme.test/careers',
			strength: 0.8,
			observed_at: '2026-02-01T00:00:00Z',
			raw: {},
		},
	],
	contacts: [
		{
			id: 'c1',
			address: 'jane@acme.test',
			method: 'page_scrape',
			kind: 'nominative',
			confidence: 0.9,
			provenance: 'https://acme.test/team',
			found_at: '2026-02-01T00:00:00Z',
		},
	],
	assessments: [
		{
			id: 'a1',
			profile_revision: 3,
			summary: 'Growing team',
			approach_angle: 'Offer tooling',
			evidence: [
				{ quote: 'We are hiring', provenance: 'https://acme.test/careers' },
			],
			model: 'model-x',
			prompt_version: 'v2',
			draft: { subject: 'Hello', body: 'Body text', status: 'pending' },
			created_at: '2026-02-01T00:00:00Z',
		},
	],
}

function renderDetail(
	overrides: Partial<Parameters<typeof LeadDetail>[0]> = {},
) {
	return render(
		<LeadDetail
			state="ready"
			errorMessage={null}
			detail={detail}
			nextStatuses={['validated', 'rejected', 'new']}
			onBack={() => {}}
			onChangeStatus={() => {}}
			changing={false}
			changeError={null}
			{...overrides}
		/>,
	)
}

describe('LeadDetail', () => {
	it('renders header, breakdown, signals, contacts and assessments', () => {
		renderDetail()
		expect(screen.getByRole('heading', { name: 'acme.test' })).toBeTruthy()
		expect(screen.getByText('Fintech')).toBeTruthy()
		expect(screen.getByText('Détail du score')).toBeTruthy()
		expect(screen.getByText('Source : crawler')).toBeTruthy()
		expect(screen.getByText('ats-fr')).toBeTruthy()
		expect(screen.getByText('jane@acme.test')).toBeTruthy()
		expect(screen.getByText('Growing team')).toBeTruthy()
		expect(screen.getByText('Modèle : model-x')).toBeTruthy()
		expect(screen.getByText('Prompt : v2')).toBeTruthy()
		expect(screen.getByText('Révision du profil : 3')).toBeTruthy()
		expect(screen.getByText('Hello')).toBeTruthy()
		expect(screen.getByText('En attente')).toBeTruthy()
	})

	it('shows the verdict and the confidence of the lead', () => {
		const { container } = renderDetail()
		const badge = container.querySelector('[data-verdict]')
		expect(badge?.getAttribute('data-verdict')).toBe('fiable')
		expect(badge?.textContent).toBe('Fiable')
		expect(screen.getByText('Confiance')).toBeTruthy()
		expect(screen.getByText(/^72,5\s%$/)).toBeTruthy()
	})

	it('renders one qualification card per agent with answers and bars', () => {
		const { container } = renderDetail()
		expect(screen.getByText('Qualification (2)')).toBeTruthy()
		const cards = container.querySelectorAll('article[data-agent]')
		expect(cards).toHaveLength(2)
		const valid = container.querySelector('article[data-agent="pertinence"]')
		expect(valid?.getAttribute('data-status')).toBe('valid')
		expect(valid?.textContent).toContain('Pertinence')
		expect(valid?.textContent).toContain('Laya (gemma_laya)')
		expect(valid?.textContent).toContain(
			"Ce lead est-il pertinent pour l'offre ?",
		)
		expect(valid?.textContent).toContain('Réponse : Moyenne')
		expect(valid?.textContent).toMatch(/confiance 80\s%/)
		expect(valid?.textContent).toMatch(/Score de l'agent : 62,5\s%/)
		const bars = valid?.querySelectorAll('[data-probability]') ?? []
		expect(Array.from(bars).map((bar) => bar.getAttribute('style'))).toEqual([
			'width: 25%;',
			'width: 50%;',
			'width: 25%;',
		])
	})

	it('shows the error of an invalid agent result without a score', () => {
		const { container } = renderDetail()
		const invalid = container.querySelector('article[data-agent="maturite"]')
		expect(invalid?.getAttribute('data-status')).toBe('invalid')
		expect(invalid?.textContent).toContain('Invalide')
		expect(invalid?.textContent).toContain('fournisseur indisponible')
		expect(invalid?.textContent).not.toContain("Score de l'agent")
		expect(invalid?.querySelectorAll('[data-probability]')).toHaveLength(0)
	})

	it('says so when no agent has answered yet', () => {
		renderDetail({
			detail: {
				...detail,
				lead: { ...detail.lead, verdict: null, confidence: null },
				qualification_results: [],
			},
		})
		expect(
			screen.getByText('Aucune qualification pour le moment.'),
		).toBeTruthy()
		expect(screen.getByText('Qualification (0)')).toBeTruthy()
	})

	it('renders evidence as a blockquote with an external provenance link', () => {
		const { container } = renderDetail()
		const quote = container.querySelector('blockquote')
		expect(quote?.textContent).toContain('We are hiring')
		const link = quote?.querySelector('a')
		expect(link?.getAttribute('href')).toBe('https://acme.test/careers')
		expect(link?.getAttribute('target')).toBe('_blank')
		expect(link?.getAttribute('rel')).toBe('noopener noreferrer')
	})

	it('only offers the allowed next statuses', () => {
		renderDetail({ nextStatuses: ['validated', 'rejected', 'new'] })
		expect(screen.getByRole('button', { name: 'Validé' })).toBeTruthy()
		expect(screen.queryByRole('button', { name: 'Contacté' })).toBeNull()
	})

	it('reports the chosen status', () => {
		const onChangeStatus = vi.fn()
		renderDetail({ onChangeStatus })
		fireEvent.click(screen.getByRole('button', { name: 'Rejeté' }))
		expect(onChangeStatus).toHaveBeenCalledWith('rejected')
	})

	it('shows the server message when a transition is refused', () => {
		renderDetail({ changeError: 'transition not allowed' })
		expect(screen.getByRole('alert').textContent).toBe('transition not allowed')
	})

	it('shows the error state', () => {
		renderDetail({ state: 'error', detail: null, errorMessage: 'not found' })
		expect(screen.getByRole('alert').textContent).toContain('not found')
	})
})
