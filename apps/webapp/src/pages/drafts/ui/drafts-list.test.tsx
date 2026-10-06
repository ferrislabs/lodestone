import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { DraftEntry, PaginationMeta } from '#/pages/drafts/lib/types'
import type { VerdictKey } from '#/pages/leads/lib/verdict'
import { DraftsList } from './drafts-list'

afterEach(cleanup)

const entry: DraftEntry = {
	assessment_id: 'a1',
	lead: {
		id: 'l1',
		domain: 'acme.test',
		score: 42,
		confidence: 81.5,
		verdict: 'fiable',
		profile: { id: 'p1', name: 'Fintech' },
	},
	draft: { subject: 'Hello Acme', body: 'Body', status: 'pending' },
	summary: 's',
	verdict_narrative: 'v',
	approach_angle: 'angle',
	evidence: [],
	evidence_verified: true,
	contact: { address: 'cto@acme.test', method: 'scraped', confidence: 0.9 },
	created_at: '2026-02-01T00:00:00Z',
}

const pagination: PaginationMeta = {
	current_page: 1,
	first_page: 1,
	is_empty: false,
	last_page: 2,
	next_page: 2,
	per_page: 20,
	prev_page: null,
	total: 30,
}

function renderList(overrides: Partial<Parameters<typeof DraftsList>[0]> = {}) {
	const props = {
		entries: [entry],
		state: 'ready' as const,
		errorMessage: null,
		profiles: [{ id: 'p1', name: 'Fintech', lead_count: 3, active: true }],
		profileId: '',
		onProfileChange: vi.fn(),
		verdicts: ['fiable', 'a_verifier', 'en_attente'] as VerdictKey[],
		onVerdictsChange: vi.fn(),
		pagination,
		onPageChange: vi.fn(),
		onOpen: vi.fn(),
		...overrides,
	}
	render(<DraftsList {...props} />)
	return props
}

describe('DraftsList', () => {
	it('shows a row with domain, profile, subject, contact and pagination', () => {
		renderList()
		expect(screen.getByText('acme.test')).toBeTruthy()
		expect(screen.getAllByText('Fintech').length).toBeGreaterThan(0)
		expect(screen.getByText('Hello Acme')).toBeTruthy()
		expect(screen.getByText('cto@acme.test')).toBeTruthy()
		expect(screen.getByText('Page 1 sur 2 - 30 brouillons')).toBeTruthy()
	})

	it('shows the verdict badge and the confidence of each draft', () => {
		renderList({
			entries: [
				entry,
				{
					...entry,
					assessment_id: 'a2',
					lead: {
						...entry.lead,
						id: 'l2',
						domain: 'globex.test',
						confidence: null,
						verdict: null,
					},
				},
			],
		})
		const badges = Array.from(document.body.querySelectorAll('[data-verdict]'))
		expect(badges.map((badge) => badge.getAttribute('data-verdict'))).toEqual([
			'fiable',
			'en_attente',
		])
		expect(screen.getByText(/^81,5\s%$/)).toBeTruthy()
		expect(screen.getByText('—')).toBeTruthy()
		expect(screen.getByRole('columnheader', { name: 'Verdict' })).toBeTruthy()
		expect(screen.getByRole('columnheader', { name: 'Confiance' })).toBeTruthy()
	})

	it('offers the verdict filter with discarded leads left out by default', () => {
		renderList()
		const box = (name: string) =>
			(screen.getByLabelText(name) as HTMLInputElement).checked
		expect(box('Fiable')).toBe(true)
		expect(box('À vérifier')).toBe(true)
		expect(box('En attente')).toBe(true)
		expect(box('Écarté')).toBe(false)
	})

	it('reports the verdicts when one is ticked or cleared', () => {
		const props = renderList()
		fireEvent.click(screen.getByLabelText('Écarté'))
		expect(props.onVerdictsChange).toHaveBeenLastCalledWith([
			'fiable',
			'a_verifier',
			'en_attente',
			'ecarte',
		])
		fireEvent.click(screen.getByLabelText('Fiable'))
		expect(props.onVerdictsChange).toHaveBeenLastCalledWith([
			'a_verifier',
			'en_attente',
		])
	})

	it('opens a draft from its domain', () => {
		const props = renderList()
		fireEvent.click(screen.getByText('acme.test'))
		expect(props.onOpen).toHaveBeenCalledWith('a1')
	})

	it('reports the chosen profile filter', () => {
		const props = renderList()
		fireEvent.change(screen.getByLabelText('Profil'), {
			target: { value: 'p1' },
		})
		expect(props.onProfileChange).toHaveBeenCalledWith('p1')
	})

	it('requests the next page', () => {
		const props = renderList()
		fireEvent.click(screen.getByText('Suivant'))
		expect(props.onPageChange).toHaveBeenCalledWith(2)
	})

	it('shows the empty state', () => {
		renderList({ entries: [] })
		expect(
			screen.getByText('Aucun brouillon en attente de relecture.'),
		).toBeTruthy()
	})

	it('shows the error state', () => {
		renderList({ entries: [], state: 'error', errorMessage: 'boom' })
		expect(screen.getByRole('alert').textContent).toContain('boom')
	})

	it('shows skeletons while loading', () => {
		renderList({ entries: [], state: 'loading', pagination: null })
		expect(document.querySelectorAll('[data-slot="skeleton"]').length).toBe(5)
	})
})
