import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { newAgent } from '#/pages/profiles/lib/agents'
import { AgentCard } from '#/pages/profiles/ui/agent-card'
import { useSourceTrigger } from './use-source-trigger'

function Harness() {
	const sourceTrigger = useSourceTrigger()
	return (
		<ul>
			<AgentCard
				agent={newAgent('search', [], ['boamp'])}
				index={0}
				problems={[]}
				note={null}
				editing={false}
				confirmingRemove={false}
				runPending={sourceTrigger.pending}
				runResults={sourceTrigger.results}
				onRunSource={sourceTrigger.trigger}
				onEdit={() => undefined}
				onEnabled={() => undefined}
				onRequestRemove={() => undefined}
				onConfirmRemove={() => undefined}
				onCancelRemove={() => undefined}
			/>
		</ul>
	)
}

function stubRun(status: number, body: unknown) {
	const calls: string[] = []
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string) => {
			calls.push(new URL(String(input)).pathname)
			return new Response(JSON.stringify(body), {
				status,
				headers: { 'Content-Type': 'application/json' },
			})
		}),
	)
	return calls
}

function renderHarness() {
	render(
		<QueryClientProvider client={new QueryClient()}>
			<Harness />
		</QueryClientProvider>,
	)
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
})

describe('agent card run action', () => {
	it('runs the source of the agent and shows it as scheduled', async () => {
		const calls = stubRun(202, { data: {} })
		renderHarness()
		fireEvent.click(screen.getByRole('button', { name: /Lancer la source/ }))
		await waitFor(() =>
			expect(screen.getByRole('status').textContent).toBe('Planifié'),
		)
		expect(calls[0]).toBe('/api/v1/sources/boamp/run')
	})

	it('shows an already running source', async () => {
		stubRun(409, {
			code: 'E_CONFLICT',
			message: 'Conflict: source is already running',
			status: 409,
		})
		renderHarness()
		fireEvent.click(screen.getByRole('button', { name: /Lancer la source/ }))
		await waitFor(() =>
			expect(screen.getByRole('status').textContent).toBe('Déjà en cours'),
		)
	})

	it('shows a translated error for a source without a job', async () => {
		stubRun(404, {
			code: 'E_NOT_FOUND',
			message: 'source has no scheduled job',
			status: 404,
		})
		renderHarness()
		fireEvent.click(screen.getByRole('button', { name: /Lancer la source/ }))
		await waitFor(() =>
			expect(screen.getByRole('status').textContent).toBe(
				"La source n'a pas de tâche planifiée",
			),
		)
	})

	it('shows the french message of a provider that is not ready instead of an already running source', async () => {
		stubRun(409, {
			code: 'E_CONFLICT',
			message:
				"Conflict: Le fournisseur « BOAMP » n'est pas prêt : configurez-le et testez-le dans le catalogue avant de lancer la source.",
			status: 409,
		})
		renderHarness()
		fireEvent.click(screen.getByRole('button', { name: /Lancer la source/ }))
		await waitFor(() =>
			expect(screen.getByRole('status').textContent).toBe(
				"Le fournisseur « BOAMP » n'est pas prêt : configurez-le et testez-le dans le catalogue avant de lancer la source.",
			),
		)
	})
})
