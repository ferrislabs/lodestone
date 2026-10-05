import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { JsonValue } from '#/lib/schema-form'
import { type AgentKind, newAgent } from '#/pages/profiles/lib/agents'
import { AgentCard } from './agent-card'

afterEach(cleanup)

const NOW = Date.parse('2026-10-05T12:00:00Z')

function renderCard(
	overrides: Partial<Parameters<typeof AgentCard>[0]> & {
		kind?: AgentKind
	} = {},
) {
	const { kind, ...rest } = overrides
	const agent: JsonValue = newAgent(kind ?? 'boamp', [])
	const props = {
		agent,
		index: 0,
		problems: [],
		note: null,
		editing: false,
		confirmingRemove: false,
		onEdit: vi.fn(),
		onEnabled: vi.fn(),
		onRequestRemove: vi.fn(),
		onConfirmRemove: vi.fn(),
		onCancelRemove: vi.fn(),
		...rest,
	}
	render(
		<ul>
			<AgentCard {...props} />
		</ul>,
	)
	return props
}

describe('AgentCard run action', () => {
	it('has no run action without a handler', () => {
		renderCard()
		expect(
			screen.queryByRole('button', { name: /Lancer la source/ }),
		).toBeNull()
	})

	it('triggers the source of the agent', () => {
		const onRunSource = vi.fn()
		renderCard({ onRunSource })
		fireEvent.click(screen.getByRole('button', { name: /Lancer la source/ }))
		expect(onRunSource).toHaveBeenCalledTimes(1)
	})

	it('disables the action while the request is pending', () => {
		renderCard({ onRunSource: vi.fn(), runPending: true })
		const button = screen.getByRole('button', { name: /Lancer la source/ })
		expect((button as HTMLButtonElement).disabled).toBe(true)
	})

	it('shows the scheduled and already running results inline', () => {
		renderCard({ onRunSource: vi.fn(), runResult: { kind: 'scheduled' } })
		expect(screen.getByRole('status').textContent).toBe('Planifié')
		cleanup()
		renderCard({ onRunSource: vi.fn(), runResult: { kind: 'running' } })
		expect(screen.getByRole('status').textContent).toBe('Déjà en cours')
	})

	it('shows an error result', () => {
		renderCard({
			onRunSource: vi.fn(),
			runResult: {
				kind: 'error',
				message: "La source n'a pas de tâche planifiée",
			},
		})
		expect(screen.getByRole('status').textContent).toBe(
			"La source n'a pas de tâche planifiée",
		)
	})
})

describe('AgentCard continuous kinds', () => {
	it('shows the listening state and last activity instead of the run button', () => {
		renderCard({
			kind: 'dns_cname',
			onRunSource: vi.fn(),
			now: NOW,
			listener: {
				state: 'listening',
				last_activity_at: '2026-10-05T11:57:00Z',
			},
		})
		expect(
			screen.queryByRole('button', { name: /Lancer la source/ }),
		).toBeNull()
		expect(
			screen.getByText('En écoute · dernière activité il y a 3 minutes'),
		).toBeTruthy()
	})

	it('says so when nothing was processed yet', () => {
		renderCard({
			kind: 'http_probe',
			now: NOW,
			listener: { state: 'idle', last_activity_at: null },
		})
		expect(
			screen.getByText('En écoute · aucune activité pour le moment'),
		).toBeTruthy()
	})

	it('shows the other listener states', () => {
		renderCard({
			kind: 'certificate_transparency',
			now: NOW,
			listener: { state: 'idle_no_agent' },
		})
		expect(screen.getByText('Inactif (aucun agent)')).toBeTruthy()
		cleanup()
		renderCard({ kind: 'certificate_transparency', now: NOW })
		expect(screen.getByText('Écoute continue')).toBeTruthy()
	})

	it('flags a silent listener', () => {
		renderCard({
			kind: 'dns_cname',
			now: NOW,
			listener: { state: 'stale', last_activity_at: null },
		})
		expect(
			screen.getByText(
				"Sans nouvelles · L'instance qui écoutait ne répond plus ; une autre prendra le relais",
			),
		).toBeTruthy()
	})

	it('keeps the run button on scheduled kinds', () => {
		renderCard({ kind: 'boamp', onRunSource: vi.fn(), now: NOW })
		expect(
			screen.getByRole('button', { name: /Lancer la source/ }),
		).toBeTruthy()
		expect(document.querySelector('[data-listener-line]')).toBeNull()
	})
})
