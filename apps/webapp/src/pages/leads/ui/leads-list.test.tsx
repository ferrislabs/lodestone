import { getCoreRowModel, useReactTable } from '@tanstack/react-table'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Lead, PaginationMeta } from '#/pages/leads/lib/types'
import { buildLeadColumns } from './lead-columns'
import { LeadsList } from './leads-list'

afterEach(cleanup)

const profile = { id: 'p1', name: 'Fintech' }

const leads: Lead[] = [
	{
		id: 'l1',
		profile,
		domain: 'acme.test',
		status: 'qualified',
		score: 42,
		confidence: 72.5,
		verdict: 'fiable',
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-01T00:00:00Z',
		updated_at: '2026-02-01T00:00:00Z',
	},
	{
		id: 'l2',
		profile,
		domain: 'globex.test',
		status: 'new',
		score: 10,
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-02T00:00:00Z',
		updated_at: '2026-02-02T00:00:00Z',
	},
]

const pagination: PaginationMeta = {
	current_page: 1,
	first_page: 1,
	is_empty: false,
	last_page: 2,
	next_page: 2,
	per_page: 25,
	prev_page: null,
	total: 40,
}

interface HarnessProps {
	data: Lead[]
	state: 'loading' | 'error' | 'ready'
	onOpen?: (id: string) => void
	onExport?: (format: 'json' | 'csv') => void
}

function Harness({ data, state, onOpen, onExport }: HarnessProps) {
	const columns = buildLeadColumns(42, onOpen ?? (() => {}))
	const table = useReactTable({
		data,
		columns,
		getCoreRowModel: getCoreRowModel(),
	})
	return (
		<LeadsList
			toolbar={<div>toolbar</div>}
			table={table}
			state={state}
			errorMessage="boom"
			pagination={pagination}
			onPageChange={() => {}}
			onExport={onExport ?? (() => {})}
			exporting={false}
			exportError={null}
			columnCount={columns.length}
			noProfile={false}
		/>
	)
}

describe('LeadsList', () => {
	it('renders one row per lead with status and score', () => {
		render(<Harness data={leads} state="ready" />)
		expect(screen.getByText('acme.test')).toBeTruthy()
		expect(screen.getByText('globex.test')).toBeTruthy()
		expect(screen.getByText('Qualifié')).toBeTruthy()
		expect(screen.getByText('42')).toBeTruthy()
		expect(screen.getByText('Page 1 sur 2 - 40 leads')).toBeTruthy()
	})

	it('shows the verdict badge and the confidence of each lead', () => {
		const { container } = render(<Harness data={leads} state="ready" />)
		const badges = Array.from(container.querySelectorAll('[data-verdict]'))
		expect(badges.map((badge) => badge.textContent)).toEqual([
			'Fiable',
			'En attente',
		])
		expect(badges.map((badge) => badge.getAttribute('data-verdict'))).toEqual([
			'fiable',
			'en_attente',
		])
		expect(screen.getByText('Verdict')).toBeTruthy()
		expect(screen.getByText(/^72,5\s%$/)).toBeTruthy()
		expect(screen.getByText('—')).toBeTruthy()
	})

	it('opens a lead when its domain is clicked', () => {
		const onOpen = vi.fn()
		render(<Harness data={leads} state="ready" onOpen={onOpen} />)
		fireEvent.click(screen.getByText('globex.test'))
		expect(onOpen).toHaveBeenCalledWith('l2')
	})

	it('requests an export in the chosen format', () => {
		const onExport = vi.fn()
		render(<Harness data={leads} state="ready" onExport={onExport} />)
		fireEvent.click(screen.getByText('Exporter en CSV'))
		expect(onExport).toHaveBeenCalledWith('csv')
	})

	it('shows the empty state', () => {
		render(<Harness data={[]} state="ready" />)
		expect(
			screen.getByText('Aucun lead ne correspond à ces filtres.'),
		).toBeTruthy()
	})

	it('shows the error state', () => {
		render(<Harness data={[]} state="error" />)
		expect(screen.getByRole('alert').textContent).toContain('boom')
	})

	it('shows skeleton rows while loading', () => {
		const { container } = render(<Harness data={[]} state="loading" />)
		expect(container.querySelectorAll('[data-slot="skeleton"]').length).toBe(6)
	})
})
