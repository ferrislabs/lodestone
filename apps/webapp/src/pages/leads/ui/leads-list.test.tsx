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
		exclusions: [],
		grouped_leads: [],
		score: 42,
		confidence: 72.5,
		llm_score: 72,
		verdict: 'fiable',
		summary:
			'Le domaine expose un Keycloak ancien et recrute un ingénieur IAM.',
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-01T00:00:00Z',
		updated_at: '2026-02-01T00:00:00Z',
	},
	{
		id: 'l2',
		profile,
		domain: 'globex.test',
		status: 'new',
		exclusions: [],
		grouped_leads: [],
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
	const columns = buildLeadColumns(onOpen ?? (() => {}))
	const table = useReactTable({
		data,
		columns,
		getCoreRowModel: getCoreRowModel(),
		getRowCanExpand: (row) => row.original.grouped_leads.length > 0,
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
			onOpenLead={onOpen ?? (() => {})}
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
		expect(screen.getByText('72 / 100')).toBeTruthy()
		expect(screen.queryByText('42')).toBeNull()
		expect(screen.getByText('Page 1 sur 2 - 40 leads')).toBeTruthy()
	})

	it('shows the verdict badge and the confidence of each lead', () => {
		const { container } = render(<Harness data={leads} state="ready" />)
		const badges = Array.from(container.querySelectorAll('[data-verdict]'))
		expect(badges.map((badge) => badge.textContent)).toEqual([
			'Fiable',
			'En attente du scoring',
		])
		expect(badges.map((badge) => badge.getAttribute('data-verdict'))).toEqual([
			'fiable',
			'en_attente',
		])
		expect(screen.getByText('Verdict')).toBeTruthy()
		expect(screen.getByText(/^72,5\s%$/)).toBeTruthy()
		expect(screen.getAllByText('—')).toHaveLength(2)
	})

	it('shows the enrichment wait in the verdict area of a pending lead', () => {
		const waiting: Lead[] = [
			{
				...leads[1],
				id: 'w1',
				domain: 'wait.test',
				enrichment_waiting: {
					waiting_reason: 'awaiting_enrichment',
					wait_until: '2026-02-02T00:30:00Z',
				},
			},
			{
				...leads[1],
				id: 'w2',
				domain: 'cert.test',
				enrichment_waiting: {
					waiting_reason: 'awaiting_enrichment_evidence',
					wait_until: null,
				},
			},
			{
				...leads[0],
				id: 'w3',
				domain: 'done.test',
				enrichment_waiting: {
					waiting_reason: 'awaiting_enrichment',
					wait_until: null,
				},
			},
		]
		const { container } = render(<Harness data={waiting} state="ready" />)
		const badges = Array.from(container.querySelectorAll('[data-verdict]'))
		expect(badges.map((badge) => badge.textContent)).toEqual([
			"En attente d'enrichissement",
			"En attente de preuve d'enrichissement",
			'Fiable',
		])
	})

	it('shows the score decided by the scoring agents out of 100', () => {
		const { container } = render(<Harness data={leads} state="ready" />)
		const bars = Array.from(
			container.querySelectorAll('[role="progressbar"][aria-label="Score"]'),
		)
		expect(bars.map((bar) => bar.getAttribute('aria-valuenow'))).toEqual([
			'72',
			null,
		])
		const cells = Array.from(
			container.querySelectorAll('[data-part="llm-score"]'),
		)
		expect(cells.map((cell) => cell.textContent)).toEqual(['72 / 100', '—'])
	})

	it('shows the opinion and evidence breakdown under the confidence', () => {
		const scored: Lead = {
			...leads[0],
			opinion: 100,
			evidence: {
				strength: 35,
				components: {
					volume: 28.4,
					diversity: 50,
					environment: 0,
					strength: 24.2,
				},
				signal_count: 1,
				independent_signals: 1,
				decayed_signals: 0.5,
				source_count: 1,
				kind_count: 1,
				signal_score: 12.1,
				only_non_production: true,
			},
		}
		const { container } = render(
			<Harness data={[scored, leads[1]]} state="ready" />,
		)
		const breakdowns = container.querySelectorAll('[data-confidence-breakdown]')
		expect(breakdowns).toHaveLength(1)
		expect(breakdowns[0]?.textContent).toMatch(
			/^Indicateurs : avis 100\s%\s×\spreuves 35\s%$/,
		)
		expect(
			container.querySelector('[title^="Indicateurs"]')?.getAttribute('title'),
		).toMatch(/preuves 35/)
	})

	it('shows the confidence bar and the summary excerpt of each lead', () => {
		const long = `${'Phrase de synthèse. '.repeat(20)}`
		const { container } = render(
			<Harness
				data={[{ ...leads[0], summary: long }, leads[1]]}
				state="ready"
			/>,
		)
		const bars = Array.from(
			container.querySelectorAll(
				'[role="progressbar"][aria-label="Confiance"]',
			),
		)
		expect(bars.map((bar) => bar.getAttribute('aria-valuenow'))).toEqual([
			'73',
			null,
		])
		const fills = container.querySelectorAll('[data-confidence-fill]')
		expect(Array.from(fills).map((fill) => fill.getAttribute('style'))).toEqual(
			['width: 73%;', 'width: 0%;'],
		)
		const excerpt = screen.getByText(/^Phrase de synthèse\./)
		expect(excerpt.textContent?.length).toBeLessThanOrEqual(140)
		expect(excerpt.textContent?.endsWith('…')).toBe(true)
		expect(screen.getByText('Pas encore de synthèse')).toBeTruthy()
	})

	it('keeps the score as a secondary column after the verdict', () => {
		render(<Harness data={leads} state="ready" />)
		const headers = screen
			.getAllByRole('columnheader')
			.map((header) => header.textContent)
		expect(headers).toEqual([
			'Domaine',
			'Verdict',
			'Confiance',
			'Synthèse',
			'Score',
			'Statut',
			'Dernière détection',
		])
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

	it('shows why a lead was excluded under its discarded badge', () => {
		const excluded: Lead = {
			...leads[0],
			verdict: 'ecarte',
			verdict_reason: 'excluded',
			exclusions: [
				{
					agent_id: 'exclusions',
					agent_name: 'Exclusions',
					question_id: 'concurrent',
					option: 'oui',
					label: 'Concurrent',
					probability: 85,
					evidence: 'Nous sommes un concurrent.',
					evidence_source: 'lead',
				},
				{
					agent_id: 'exclusions',
					agent_name: 'Exclusions',
					question_id: 'client_actuel',
					option: 'oui',
					label: 'Client actuel',
					probability: 70,
					evidence: 'Client depuis 2024.',
					evidence_source: 'lead',
				},
			],
		}
		const { container } = render(
			<Harness data={[excluded, leads[1]]} state="ready" />,
		)
		const badges = Array.from(container.querySelectorAll('[data-verdict]'))
		expect(badges.map((badge) => badge.textContent)).toEqual([
			'Écarté',
			'En attente du scoring',
		])
		const hints = container.querySelectorAll('[data-part="exclusion"]')
		expect(hints).toHaveLength(1)
		expect(hints[0]?.textContent).toBe('Exclu : Concurrent (+1)')
		expect(hints[0]?.parentElement?.getAttribute('title')).toBe(
			"Exclu par l'agent « Exclusions » : Concurrent (85 %)\nExclu par l'agent « Exclusions » : Client actuel (70 %)",
		)
	})
	describe('grouped by company', () => {
		const grouped: Lead[] = [
			{
				...leads[0],
				grouped_leads: [
					{
						id: 'l9',
						domain: 'acme.fr',
						status: 'new',
						verdict: 'a_verifier',
						confidence: 41,
					},
					{
						id: 'l10',
						domain: 'acme.io',
						status: 'qualified',
						verdict: null,
						confidence: null,
					},
				],
			},
			leads[1],
		]

		it('shows a +N domaines badge only on rows that stand for siblings', () => {
			const { container } = render(<Harness data={grouped} state="ready" />)
			const toggles = container.querySelectorAll('[data-group-toggle]')
			expect(toggles).toHaveLength(1)
			expect(toggles[0]?.textContent).toBe('+2 domaines')
			expect(toggles[0]?.getAttribute('aria-expanded')).toBe('false')
			expect(screen.queryByText('acme.fr')).toBeNull()
		})

		it('uses the singular for one more domain', () => {
			const [first] = grouped
			if (!first) throw new Error('fixture')
			const one: Lead = {
				...first,
				grouped_leads: first.grouped_leads.slice(0, 1),
			}
			render(<Harness data={[one]} state="ready" />)
			expect(screen.getByText('+1 domaine')).toBeTruthy()
		})

		it('expands the sibling domains with verdict and confidence and opens one', () => {
			const onOpen = vi.fn()
			const { container } = render(
				<Harness data={grouped} state="ready" onOpen={onOpen} />,
			)
			fireEvent.click(screen.getByText('+2 domaines'))
			const group = container.querySelector('[data-group="l1"]')
			expect(group?.textContent).toContain('acme.fr')
			expect(group?.textContent).toContain('acme.io')
			expect(group?.textContent).toContain('À vérifier')
			expect(group?.textContent).toMatch(/41\s%/)
			expect(
				container
					.querySelector('[data-group-toggle]')
					?.getAttribute('aria-expanded'),
			).toBe('true')
			fireEvent.click(screen.getByText('acme.fr'))
			expect(onOpen).toHaveBeenCalledWith('l9')
			fireEvent.click(screen.getByText('+2 domaines'))
			expect(container.querySelector('[data-group="l1"]')).toBeNull()
		})
	})
})
