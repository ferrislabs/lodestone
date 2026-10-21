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

import type { Source } from '#/pages/sources/lib/types'
import { SourcesFeature } from './sources-feature'

const FUTURE = '2099-01-01T00:00:00Z'

const router = vi.hoisted(() => ({ navigate: vi.fn() }))

vi.mock('@tanstack/react-router', () => ({
	useNavigate: () => router.navigate,
}))

function source(name: string, overrides: Partial<Source> = {}): Source {
	return {
		source: name,
		kind: 'source.run',
		label_key: `source.${name}`,
		status: 'pending',
		mode: 'scheduled',
		attempts: 0,
		next_run_at: FUTURE,
		last_runs: [],
		...overrides,
	}
}

interface Backend {
	sources: Source[]
	runStatus: number
	runCalls: string[]
	listCalls: number
}

function json(body: unknown, status: number): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' },
	})
}

function install(backend: Backend) {
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string, init?: RequestInit) => {
			const url = new URL(String(input))
			if (init?.method === 'POST') {
				backend.runCalls.push(url.pathname)
				if (backend.runStatus === 409) {
					return json(
						{
							code: 'E_CONFLICT',
							message: 'Conflict: source is already running',
							status: 409,
						},
						409,
					)
				}
				return json({ data: source('boamp') }, 202)
			}
			backend.listCalls += 1
			return json({ data: backend.sources }, 200)
		}),
	)
}

function renderFeature() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	})
	render(
		<QueryClientProvider client={client}>
			<SourcesFeature />
		</QueryClientProvider>,
	)
}

let backend: Backend

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	router.navigate.mockReset()
	backend = {
		sources: [source('boamp'), source('decp', { status: 'running' })],
		runStatus: 202,
		runCalls: [],
		listCalls: 0,
	}
	install(backend)
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
})

async function boampRow() {
	const row = await screen.findByRole('row', { name: /BOAMP/ })
	return within(row)
}

describe('SourcesFeature', () => {
	it('lists the sources and runs one now', async () => {
		renderFeature()
		const row = await boampRow()
		backend.sources = [
			source('boamp', { next_run_at: '2020-01-01T00:00:00Z' }),
			source('decp', { status: 'running' }),
		]

		fireEvent.click(row.getByRole('button', { name: 'Lancer maintenant' }))

		await waitFor(() =>
			expect(backend.runCalls).toEqual(['/api/v1/sources/boamp/run']),
		)
		await waitFor(() =>
			expect(
				row.getByRole('button', { name: 'Planifié maintenant' }),
			).toBeTruthy(),
		)
		await waitFor(() => expect(backend.listCalls).toBeGreaterThan(1))
	})

	it('shows the conflict of an already running source in french', async () => {
		backend.runStatus = 409
		renderFeature()
		const row = await boampRow()

		fireEvent.click(row.getByRole('button', { name: 'Lancer maintenant' }))

		await waitFor(() =>
			expect(row.getByRole('alert').textContent).toBe(
				'La source est déjà en cours',
			),
		)
		expect(row.getByRole('button', { name: 'Lancer maintenant' })).toBeTruthy()
	})

	it('shows the provider badge and opens the catalog page of the provider', async () => {
		backend.sources = [
			source('boamp', {
				provider: { key: 'boamp', name: 'BOAMP', status: 'failing' },
			}),
		]
		renderFeature()
		const row = await boampRow()

		expect(row.getByText('Provider non prêt')).toBeTruthy()
		fireEvent.click(
			row.getByRole('button', { name: 'Ouvrir BOAMP dans le catalogue' }),
		)

		expect(router.navigate).toHaveBeenCalledWith({
			to: '/sources/providers/$key',
			params: { key: 'boamp' },
		})
		expect(
			(
				row.getByRole('button', {
					name: 'Lancer maintenant',
				}) as HTMLButtonElement
			).disabled,
		).toBe(true)
	})

	it('shows a failure to load', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () =>
				json(
					{ code: 'E_INTERNAL', message: 'Internal server error', status: 500 },
					500,
				),
			),
		)
		renderFeature()
		expect((await screen.findByRole('alert')).textContent).toBe(
			'Erreur interne du serveur',
		)
	})

	it('polls every five seconds while a source runs', async () => {
		vi.useFakeTimers({ shouldAdvanceTime: true })
		try {
			renderFeature()
			await boampRow()
			const before = backend.listCalls
			await vi.advanceTimersByTimeAsync(5_100)
			await waitFor(() => expect(backend.listCalls).toBeGreaterThan(before))
		} finally {
			vi.useRealTimers()
		}
	})

	it('does not poll when nothing runs', async () => {
		vi.useFakeTimers({ shouldAdvanceTime: true })
		try {
			backend.sources = [source('boamp')]
			renderFeature()
			await boampRow()
			const before = backend.listCalls
			await vi.advanceTimersByTimeAsync(11_000)
			expect(backend.listCalls).toBe(before)
		} finally {
			vi.useRealTimers()
		}
	})
})
