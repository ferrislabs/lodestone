import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { CONFLICT_MESSAGE } from '#/pages/drafts/lib/conflict'
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
	draft: {
		subject: 'Hello Acme',
		body: 'Body text',
		status: 'pending',
		revision: 3,
	},
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
let current: DraftEntry
let writeText: ReturnType<typeof vi.fn>
let respond: (call: Call) => Response | null

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' },
	})
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	calls = []
	current = ENTRY
	writeText = vi.fn(async () => {})
	respond = () => null
	Object.defineProperty(navigator, 'clipboard', {
		value: { writeText },
		configurable: true,
	})
	router.navigate.mockReset()
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string, init?: RequestInit) => {
			const url = new URL(String(input))
			const method = init?.method ?? 'GET'
			const body = typeof init?.body === 'string' ? JSON.parse(init.body) : null
			const call = { method, path: url.pathname, body }
			calls.push(call)
			return respond(call) ?? json({ data: current })
		}),
	)
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
	Reflect.deleteProperty(navigator, 'clipboard')
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

function approvals(): Call[] {
	return calls.filter((call) => call.path.endsWith('/approve'))
}

function edits(): Call[] {
	return calls.filter((call) => call.method === 'PATCH')
}

function loads(): Call[] {
	return calls.filter(
		(call) => call.method === 'GET' && call.path === '/api/v1/drafts/a1',
	)
}

function failWith(method: string, status: number, message: string) {
	respond = (call) =>
		call.method === method ? json({ message }, status) : null
}

async function rejectWithOther() {
	fireEvent.click(await screen.findByRole('button', { name: 'Rejeter' }))
	fireEvent.click(screen.getByLabelText('Autre'))
	fireEvent.change(screen.getByLabelText('Précision'), {
		target: { value: 'déjà en discussion' },
	})
	fireEvent.click(screen.getByText('Confirmer le rejet'))
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
			expected_revision: 3,
		})
		await waitFor(() =>
			expect(router.navigate).toHaveBeenCalledWith({
				to: '/drafts',
				search: {},
			}),
		)
	})
})

describe('DraftReviewFeature copy actions', () => {
	it('copies the edited text of a pending draft, not the original', async () => {
		current = {
			...ENTRY,
			contact: { address: 'cto@acme.test', method: 'scraped', confidence: 0.9 },
		}
		renderFeature()
		fireEvent.change(await screen.findByLabelText('Objet'), {
			target: { value: 'Edited subject' },
		})
		fireEvent.change(screen.getByLabelText('Corps'), {
			target: { value: 'Edited body' },
		})
		fireEvent.click(screen.getByRole('button', { name: "Copier l'objet" }))
		expect(writeText).toHaveBeenLastCalledWith('Edited subject')
		fireEvent.click(screen.getByRole('button', { name: 'Copier le corps' }))
		expect(writeText).toHaveBeenLastCalledWith('Edited body')
		fireEvent.click(screen.getByRole('button', { name: 'Copier le brouillon' }))
		expect(writeText).toHaveBeenLastCalledWith('Edited subject\n\nEdited body')
		expect(
			screen
				.getByRole('link', { name: 'Ouvrir dans la messagerie' })
				.getAttribute('href'),
		).toBe('mailto:cto@acme.test?subject=Edited%20subject&body=Edited%20body')
	})

	it('offers no mail link to a pending draft without a contact', async () => {
		renderFeature()
		await screen.findByLabelText('Objet')
		expect(screen.queryByRole('link', { name: /messagerie/ })).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: 'Copier le corps' }))
		expect(writeText).toHaveBeenLastCalledWith('Body text')
	})

	it.each([
		'approved',
		'rejected',
	] as const)('shows a %s draft read-only and copies it', async (status) => {
		current = {
			...ENTRY,
			contact: { address: 'cto@acme.test', method: 'scraped', confidence: 0.9 },
			draft: { subject: 'Hello Acme', body: 'Body text', status, revision: 3 },
		}
		renderFeature()
		expect(await screen.findByText('Hello Acme')).toBeTruthy()
		expect(screen.getByText('Body text')).toBeTruthy()
		expect(screen.queryAllByRole('textbox')).toHaveLength(0)
		expect(screen.queryByRole('button', { name: 'Rejeter' })).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: 'Copier le brouillon' }))
		expect(writeText).toHaveBeenLastCalledWith('Hello Acme\n\nBody text')
		expect(
			screen
				.getByRole('link', { name: 'Ouvrir dans la messagerie' })
				.getAttribute('href'),
		).toBe('mailto:cto@acme.test?subject=Hello%20Acme&body=Body%20text')
	})
})

describe('DraftReviewFeature revision', () => {
	it('sends the loaded revision with an edit', async () => {
		renderFeature()
		fireEvent.change(await screen.findByLabelText('Objet'), {
			target: { value: 'New subject' },
		})
		fireEvent.click(
			screen.getByRole('button', { name: 'Enregistrer les modifications' }),
		)
		await waitFor(() => expect(edits()).toHaveLength(1))
		expect(edits()[0].path).toBe('/api/v1/drafts/a1')
		expect(edits()[0].body).toEqual({
			subject: 'New subject',
			body: 'Body text',
			expected_revision: 3,
		})
	})

	it('sends the loaded revision with an approval', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: /^Approuver/ }))
		await waitFor(() => expect(approvals()).toHaveLength(1))
		expect(approvals()[0].path).toBe('/api/v1/drafts/a1/approve')
		expect(approvals()[0].body).toEqual({ expected_revision: 3 })
	})

	it('sends the loaded revision with a rejection', async () => {
		renderFeature()
		await rejectWithOther()
		await waitFor(() => expect(rejects()).toHaveLength(1))
		expect(rejects()[0].body).toMatchObject({ expected_revision: 3 })
	})

	it('sends the refreshed revision when retrying after a conflict', async () => {
		respond = (call) => {
			if (call.method !== 'PATCH') return null
			current = {
				...ENTRY,
				summary: 'Summary text v4',
				draft: { ...ENTRY.draft, revision: 4 },
			}
			respond = () => null
			return json({ message: 'Conflict: draft revision mismatch' }, 409)
		}
		renderFeature()
		fireEvent.change(await screen.findByLabelText('Objet'), {
			target: { value: 'New subject' },
		})
		fireEvent.click(
			screen.getByRole('button', { name: 'Enregistrer les modifications' }),
		)
		await screen.findByText(CONFLICT_MESSAGE)
		await screen.findByText('Summary text v4')
		fireEvent.change(screen.getByLabelText('Objet'), {
			target: { value: 'Newer subject' },
		})
		fireEvent.click(
			screen.getByRole('button', { name: 'Enregistrer les modifications' }),
		)
		await waitFor(() => expect(edits()).toHaveLength(2))
		expect(edits()[1].body).toMatchObject({ expected_revision: 4 })
	})
})

describe('DraftReviewFeature conflicts', () => {
	it('shows the conflict message on approve and reloads the draft', async () => {
		failWith('POST', 409, 'Conflict: draft revision mismatch')
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: /^Approuver/ }))
		expect(await screen.findByText(CONFLICT_MESSAGE)).toBeTruthy()
		await waitFor(() => expect(loads()).toHaveLength(2))
		expect(router.navigate).not.toHaveBeenCalled()
	})

	it('shows the conflict message on edit and keeps the typed text', async () => {
		failWith('PATCH', 409, 'Conflict: draft revision mismatch')
		renderFeature()
		fireEvent.change(await screen.findByLabelText('Objet'), {
			target: { value: 'Typed subject' },
		})
		fireEvent.change(screen.getByLabelText('Corps'), {
			target: { value: 'Typed body' },
		})
		fireEvent.click(
			screen.getByRole('button', { name: 'Enregistrer les modifications' }),
		)
		expect(await screen.findByText(CONFLICT_MESSAGE)).toBeTruthy()
		await waitFor(() => expect(loads()).toHaveLength(2))
		expect((screen.getByLabelText('Objet') as HTMLInputElement).value).toBe(
			'Typed subject',
		)
		expect((screen.getByLabelText('Corps') as HTMLTextAreaElement).value).toBe(
			'Typed body',
		)
	})

	it('shows the conflict message in the rejection dialog', async () => {
		failWith('POST', 409, 'Conflict: draft revision mismatch')
		renderFeature()
		await rejectWithOther()
		const dialog = await screen.findByRole('dialog')
		expect(await within(dialog).findByText(CONFLICT_MESSAGE)).toBeTruthy()
		await waitFor(() => expect(loads()).toHaveLength(2))
		expect(router.navigate).not.toHaveBeenCalled()
	})

	it('keeps the server message for an error that is not a conflict', async () => {
		failWith('POST', 500, 'Internal server error')
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: /^Approuver/ }))
		expect(await screen.findByText('Erreur interne du serveur')).toBeTruthy()
		expect(screen.queryByText(CONFLICT_MESSAGE)).toBeNull()
		expect(loads()).toHaveLength(1)
	})
})
