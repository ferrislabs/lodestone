import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { JsonValue } from '#/lib/schema-form'
import { newAgent } from '#/pages/profiles/lib/agents'
import { AgentCard } from './agent-card'

afterEach(cleanup)

function renderCard(overrides: Partial<Parameters<typeof AgentCard>[0]> = {}) {
	const agent: JsonValue = newAgent('boamp', [])
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
		...overrides,
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
