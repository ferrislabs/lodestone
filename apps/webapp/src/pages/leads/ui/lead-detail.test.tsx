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
	},
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
