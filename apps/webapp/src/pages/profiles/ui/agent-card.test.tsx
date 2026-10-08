import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { JsonValue } from '#/lib/schema-form'
import { type AgentKind, newAgent } from '#/pages/profiles/lib/agents'
import { summary } from '#/pages/source-providers/lib/fixtures'
import { AgentCard } from './agent-card'

afterEach(cleanup)

const NOW = Date.parse('2026-10-05T12:00:00Z')

function renderCard(
	overrides: Partial<Parameters<typeof AgentCard>[0]> & {
		kind?: AgentKind
	} = {},
) {
	const { kind, ...rest } = overrides
	const agent: JsonValue = newAgent(kind ?? 'search', [], ['boamp'])
	const props = {
		agent,
		index: 0,
		problems: [],
		note: null,
		highlighted: false,
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

describe('AgentCard content', () => {
	it('titles the card with the agent name and badges its type', () => {
		renderCard({
			agent: { ...newAgent('search', [], ['boamp']), name: 'Marchés IAM' },
		})
		expect(screen.getByRole('heading', { name: 'Marchés IAM' })).toBeTruthy()
		expect(screen.getByText('Recherche')).toBeTruthy()
		cleanup()
		renderCard({ kind: 'certificates' })
		expect(screen.getAllByText('Certificats').length).toBeGreaterThan(0)
		expect(screen.queryByText('Signal')).toBeNull()
		cleanup()
		renderCard({ kind: 'dns' })
		expect(screen.getAllByText('DNS').length).toBeGreaterThan(0)
	})

	it('shows the providers of a search agent as logos with a status border', () => {
		renderCard({
			agent: newAgent('search', [], ['boamp', 'ted']),
			catalog: [
				summary({ key: 'boamp', name: 'BOAMP', status: 'ready' }),
				summary({ key: 'ted', name: 'TED', status: 'failing' }),
			],
		})
		const row = screen.getByRole('list', { name: "Sources de l'agent" })
		const logos = row.querySelectorAll('[data-provider-logo]')
		expect([...logos].map((logo) => logo.getAttribute('title'))).toEqual([
			'BOAMP · Prêt',
			'TED · En échec',
		])
		expect(logos[0]?.className).toContain('border-emerald-500')
		expect(screen.getByRole('img', { name: 'BOAMP' })).toBeTruthy()
	})

	it('counts the labels and patterns of signal agents', () => {
		renderCard({
			agent: { ...newAgent('certificates', []), labels: ['auth', 'sso'] },
		})
		expect(screen.getByText('2 labels')).toBeTruthy()
		expect(
			screen.queryByRole('list', { name: "Sources de l'agent" }),
		).toBeNull()
		cleanup()
		renderCard({ agent: { ...newAgent('dns', []), patterns: [] } })
		expect(screen.getByText('0 motif CNAME')).toBeTruthy()
	})

	it('edits and removes the agent', () => {
		const props = renderCard()
		fireEvent.click(screen.getByRole('button', { name: /Modifier/ }))
		fireEvent.click(screen.getByRole('button', { name: /Supprimer/ }))
		expect(props.onEdit).toHaveBeenCalled()
		expect(props.onRequestRemove).toHaveBeenCalled()
	})
})

describe('AgentCard run action', () => {
	it('has no run action without a handler', () => {
		renderCard()
		expect(screen.queryByRole('button', { name: /Lancer l'agent/ })).toBeNull()
	})

	it('runs the whole agent from one button', () => {
		const onRunAgent = vi.fn()
		renderCard({ onRunAgent })
		fireEvent.click(screen.getByRole('button', { name: /Lancer l'agent/ }))
		expect(onRunAgent).toHaveBeenCalledTimes(1)
		expect(screen.queryByRole('button', { name: /Lancer BOAMP/ })).toBeNull()
	})

	it('disables the action while a request is pending', () => {
		renderCard({
			onRunAgent: vi.fn(),
			run: { pending: true, message: null, error: false },
		})
		const button = screen.getByRole('button', { name: /Lancer l'agent/ })
		expect((button as HTMLButtonElement).disabled).toBe(true)
	})

	it('disables the action without any provider', () => {
		renderCard({ agent: newAgent('search', []), onRunAgent: vi.fn() })
		const button = screen.getByRole('button', { name: /Lancer l'agent/ })
		expect((button as HTMLButtonElement).disabled).toBe(true)
	})

	it('shows one combined result', () => {
		renderCard({
			onRunAgent: vi.fn(),
			run: {
				pending: false,
				message: 'Agent lancé : 2 sources planifiées',
				error: false,
			},
		})
		expect(screen.getByRole('status').textContent).toBe(
			'Agent lancé : 2 sources planifiées',
		)
		cleanup()
		renderCard({
			onRunAgent: vi.fn(),
			run: { pending: false, message: 'TED : Non prêt', error: true },
		})
		expect(screen.getByRole('status').className).toContain('text-destructive')
	})
})

describe('AgentCard continuous kinds', () => {
	it('shows the listening state and last activity instead of the run button', () => {
		renderCard({
			kind: 'dns',
			onRunAgent: vi.fn(),
			now: NOW,
			listener: {
				state: 'listening',
				last_activity_at: '2026-10-05T11:57:00Z',
			},
		})
		expect(screen.queryByRole('button', { name: /Lancer l'agent/ })).toBeNull()
		expect(
			screen.getByText('En écoute · dernière activité il y a 3 minutes'),
		).toBeTruthy()
	})

	it('says so when nothing was processed yet', () => {
		renderCard({
			kind: 'certificates',
			now: NOW,
			listener: { state: 'idle', last_activity_at: null },
		})
		expect(
			screen.getByText('En écoute · aucune activité pour le moment'),
		).toBeTruthy()
	})

	it('shows the other listener states', () => {
		renderCard({
			kind: 'certificates',
			now: NOW,
			listener: { state: 'idle_no_agent' },
		})
		expect(screen.getByText('Inactif (aucun agent)')).toBeTruthy()
		cleanup()
		renderCard({ kind: 'certificates', now: NOW })
		expect(screen.getByText('Écoute continue')).toBeTruthy()
	})

	it('flags a silent listener', () => {
		renderCard({
			kind: 'dns',
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
		renderCard({ onRunAgent: vi.fn(), now: NOW })
		expect(screen.getByRole('button', { name: /Lancer l'agent/ })).toBeTruthy()
		expect(document.querySelector('[data-listener-line]')).toBeNull()
	})
})
