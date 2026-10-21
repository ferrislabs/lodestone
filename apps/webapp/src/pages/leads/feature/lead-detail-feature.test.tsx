import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { LeadDetailData } from '#/pages/leads/lib/types'
import { LeadDetailFeature } from './lead-detail-feature'

vi.mock('@tanstack/react-router', () => ({
	useParams: () => ({ leadId: 'l1' }),
	useNavigate: () => vi.fn(),
}))

const DETAIL: LeadDetailData = {
	lead: {
		id: 'l1',
		profile: { id: 'p1', name: 'Fintech' },
		domain: 'acme.test',
		status: 'qualified',
		exclusions: [],
		grouped_leads: [],
		score: 42,
		confidence: 66,
		verdict: 'a_verifier',
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-01T00:00:00Z',
		updated_at: '2026-02-01T00:00:00Z',
	},
	qualification_results: [],
	scoring_results: [],
	assessments: [],
	facts: {
		distinct_hosts: 0,
		distinct_subdomains: 0,
		first_seen_at: '2026-01-01T00:00:00Z',
		hosts: [],
		last_seen_at: '2026-02-01T00:00:00Z',
		only_non_production: false,
		production_hosts: 0,
		signal_count: 0,
		independent_signals: 0,
		kinds: [],
		sibling_domains: [],
		source_count: 0,
		sources: [],
	},
	score_breakdown: [],
	signals: [],
	contacts: [],
}

interface Call {
	method: string
	path: string
	body: unknown
}

let calls: Call[]
let statusResponse: () => Response

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' },
	})
}

function install() {
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string, init?: RequestInit) => {
			const url = new URL(String(input))
			const method = init?.method ?? 'GET'
			const body = typeof init?.body === 'string' ? JSON.parse(init.body) : null
			calls.push({ method, path: url.pathname, body })
			if (method === 'PATCH') return statusResponse()
			return json({ data: DETAIL })
		}),
	)
}

function renderFeature() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	})
	render(
		<QueryClientProvider client={client}>
			<LeadDetailFeature />
		</QueryClientProvider>,
	)
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	calls = []
	statusResponse = () => json({ data: DETAIL.lead })
	install()
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
})

function patches(): Call[] {
	return calls.filter((call) => call.method === 'PATCH')
}

describe('LeadDetailFeature rejection', () => {
	it('opens a dialog instead of rejecting straight away', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Rejeté' }))
		expect(screen.getByRole('dialog')).toBeTruthy()
		expect(patches()).toHaveLength(0)
	})

	it('moves to another status without asking for a reason', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Validé' }))
		await waitFor(() => expect(patches()).toHaveLength(1))
		expect(patches()[0].body).toEqual({ status: 'validated' })
		expect(screen.queryByRole('dialog')).toBeNull()
	})

	it('refuses to confirm without a reason and without a note for autre', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Rejeté' }))
		fireEvent.click(screen.getByText('Confirmer le rejet'))
		expect(screen.getByText('Choisissez une raison de rejet')).toBeTruthy()
		fireEvent.click(screen.getByLabelText('Autre'))
		fireEvent.click(screen.getByText('Confirmer le rejet'))
		expect(
			screen.getByText('Précisez la raison quand elle est « Autre »'),
		).toBeTruthy()
		expect(patches()).toHaveLength(0)
	})

	it('sends the reason and the note then closes the dialog', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Rejeté' }))
		fireEvent.click(
			screen.getByLabelText(
				'Hébergeur ou agence (le signal concerne un client)',
			),
		)
		fireEvent.change(screen.getByLabelText('Précision (facultatif)'), {
			target: { value: '  agence web  ' },
		})
		fireEvent.click(screen.getByText('Confirmer le rejet'))
		await waitFor(() => expect(patches()).toHaveLength(1))
		expect(patches()[0].path).toBe('/api/v1/leads/l1/status')
		expect(patches()[0].body).toEqual({
			status: 'rejected',
			reason: 'hebergeur_agence',
			note: 'agence web',
		})
		await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
	})

	it('shows the server refusal inside the dialog', async () => {
		statusResponse = () =>
			json(
				{
					code: 'E_UNPROCESSABLE_ENTITY',
					message: 'Request payload rejected',
					status: 422,
					details: [{ path: 'reason', message: 'is required to reject' }],
				},
				422,
			)
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Rejeté' }))
		fireEvent.click(screen.getByLabelText('Concurrent'))
		fireEvent.click(screen.getByText('Confirmer le rejet'))
		expect(
			await screen.findByText('Choisissez une raison de rejet'),
		).toBeTruthy()
		expect(screen.getByRole('dialog')).toBeTruthy()
	})

	it('forgets the choice when the dialog is cancelled', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Rejeté' }))
		fireEvent.click(screen.getByLabelText('Concurrent'))
		fireEvent.click(screen.getByText('Annuler'))
		expect(screen.queryByRole('dialog')).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: 'Rejeté' }))
		expect(
			(screen.getByLabelText('Concurrent') as HTMLInputElement).checked,
		).toBe(false)
	})
})
