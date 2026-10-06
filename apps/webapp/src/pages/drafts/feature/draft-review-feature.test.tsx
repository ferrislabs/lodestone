import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftReviewFeature } from './draft-review-feature'

const router = vi.hoisted(() => ({ navigate: vi.fn() }))

vi.mock('@tanstack/react-router', () => ({
	useParams: () => ({ assessmentId: 'a1' }),
	useNavigate: () => router.navigate,
}))

const ENTRY: DraftEntry = {
	assessment_id: 'a1',
	lead: {
		id: 'l1',
		domain: 'acme.test',
		score: 42,
		confidence: 66,
		verdict: 'a_verifier',
		profile: { id: 'p1', name: 'Fintech' },
	},
	draft: { subject: 'Hello Acme', body: 'Body text', status: 'pending' },
	summary: 'Summary text',
	verdict_narrative: 'Narrative text',
	approach_angle: 'Angle text',
	evidence: [],
	evidence_verified: true,
	contact: null,
	created_at: '2026-02-01T00:00:00Z',
}

interface Call {
	method: string
	path: string
	body: unknown
}

let calls: Call[]

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' },
	})
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	calls = []
	router.navigate.mockReset()
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string, init?: RequestInit) => {
			const url = new URL(String(input))
			const method = init?.method ?? 'GET'
			const body = typeof init?.body === 'string' ? JSON.parse(init.body) : null
			calls.push({ method, path: url.pathname, body })
			return json({ data: ENTRY })
		}),
	)
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
})

function renderFeature() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	})
	render(
		<QueryClientProvider client={client}>
			<DraftReviewFeature />
		</QueryClientProvider>,
	)
}

function rejects(): Call[] {
	return calls.filter((call) => call.path.endsWith('/reject'))
}

describe('DraftReviewFeature rejection', () => {
	it('opens a dialog from the Rejeter button', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Rejeter' }))
		expect(screen.getByRole('dialog')).toBeTruthy()
		expect(screen.getByText('Rejeter ce brouillon')).toBeTruthy()
		expect(rejects()).toHaveLength(0)
	})

	it('requires a reason before rejecting', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Rejeter' }))
		fireEvent.click(screen.getByText('Confirmer le rejet'))
		expect(screen.getByText('Choisissez une raison de rejet')).toBeTruthy()
		expect(rejects()).toHaveLength(0)
	})

	it('sends the reason and the note and goes back to the queue', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Rejeter' }))
		fireEvent.click(screen.getByLabelText('Autre'))
		fireEvent.change(screen.getByLabelText('Précision'), {
			target: { value: 'déjà en discussion' },
		})
		fireEvent.click(screen.getByText('Confirmer le rejet'))
		await waitFor(() => expect(rejects()).toHaveLength(1))
		expect(rejects()[0].path).toBe('/api/v1/drafts/a1/reject')
		expect(rejects()[0].body).toEqual({
			reason: 'autre',
			note: 'déjà en discussion',
		})
		await waitFor(() =>
			expect(router.navigate).toHaveBeenCalledWith({
				to: '/drafts',
				search: {},
			}),
		)
	})
})
