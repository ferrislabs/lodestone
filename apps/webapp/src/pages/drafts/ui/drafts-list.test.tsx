import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { DraftEntry, PaginationMeta } from '#/pages/drafts/lib/types'
import { DraftsList } from './drafts-list'

afterEach(cleanup)

const entry: DraftEntry = {
	assessment_id: 'a1',
	lead: {
		id: 'l1',
		domain: 'acme.test',
		score: 42,
		profile: { id: 'p1', name: 'Fintech' },
	},
	draft: { subject: 'Hello Acme', body: 'Body', status: 'pending' },
	summary: 's',
	approach_angle: 'angle',
	evidence: [],
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
		expect(screen.getByText('Page 1 of 2 - 30 drafts')).toBeTruthy()
	})

	it('opens a draft from its domain', () => {
		const props = renderList()
		fireEvent.click(screen.getByText('acme.test'))
		expect(props.onOpen).toHaveBeenCalledWith('a1')
	})

	it('reports the chosen profile filter', () => {
		const props = renderList()
		fireEvent.change(screen.getByLabelText('Profile'), {
			target: { value: 'p1' },
		})
		expect(props.onProfileChange).toHaveBeenCalledWith('p1')
	})

	it('requests the next page', () => {
		const props = renderList()
		fireEvent.click(screen.getByText('Next'))
		expect(props.onPageChange).toHaveBeenCalledWith(2)
	})

	it('shows the empty state', () => {
		renderList({ entries: [] })
		expect(screen.getByText('No drafts waiting for review.')).toBeTruthy()
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
