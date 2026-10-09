import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { TimelineRound } from '#/pages/leads/lib/timeline'
import { LeadTimeline } from './lead-timeline'

afterEach(cleanup)

const ROUNDS: TimelineRound[] = [
	{
		number: 2,
		trigger: 'new_information',
		sources: ['http_probe'],
		source_labels: ['http_probe'],
		reconstructed: false,
		started_at: '2026-10-09T09:10:00Z',
		events: [
			{
				id: 'a',
				round: 2,
				step: 'qualification',
				kind: 'agent',
				status: 'failed',
				actor: { agent_id: 'stack', agent_name: 'Stack', model: 'qwen' },
				detail: { agent_score: null },
				started_at: '2026-10-09T09:10:00Z',
				finished_at: '2026-10-09T09:10:02Z',
				duration_ms: 2_400,
				error: "Échec de l'appel au modèle : timeout",
				reconstructed: false,
			},
			{
				id: 'b',
				round: 2,
				step: 'detection',
				kind: 'signal',
				status: 'succeeded',
				actor: { source: 'http_probe' },
				detail: { value: 'keycloak' },
				started_at: '2026-10-09T09:05:00Z',
				reconstructed: false,
			},
		],
	},
	{
		number: 1,
		trigger: 'detection',
		sources: [],
		source_labels: [],
		reconstructed: true,
		events: [
			{
				id: 'c',
				round: 1,
				step: 'aggregation',
				kind: 'summary',
				status: 'succeeded',
				actor: {},
				detail: { reconstructed: true },
				started_at: '2026-10-01T09:00:00Z',
				reconstructed: true,
			},
		],
	},
]

function renderTimeline(
	overrides: Partial<Parameters<typeof LeadTimeline>[0]> = {},
) {
	const onToggleStep = vi.fn()
	const onClearSteps = vi.fn()
	render(
		<LeadTimeline
			state="ready"
			errorMessage={null}
			rounds={ROUNDS}
			inFlight={[]}
			steps={['detection', 'qualification', 'aggregation']}
			selected={[]}
			onToggleStep={onToggleStep}
			onClearSteps={onClearSteps}
			{...overrides}
		/>,
	)
	return { onToggleStep, onClearSteps }
}

describe('LeadTimeline', () => {
	it('titles each round with its trigger, newest first', () => {
		renderTimeline()
		const titles = screen
			.getAllByRole('heading', { level: 3 })
			.map((heading) => heading.textContent)
		expect(titles[0]).toBe(
			'Évaluation n°2 — déclenchée par : nouvelles informations (Sonde HTTP)',
		)
		expect(titles[1]).toContain('Évaluation n°1 — déclenchée par : détection')
		expect(titles[1]).toContain('reconstitué')
	})

	it('shows the actor, model, status, duration and error of an event', () => {
		renderTimeline()
		const agent = document.querySelector<HTMLElement>('[data-event="agent"]')
		expect(agent?.textContent).toContain('Stack · qwen')
		expect(agent?.textContent).toContain('Échec')
		expect(agent?.textContent).toContain('2,4 s')
		expect(agent?.textContent).toContain('timeout')
		const signal = document.querySelector<HTMLElement>('[data-event="signal"]')
		expect(signal?.textContent).toContain('Sonde HTTP')
		expect(signal?.textContent).toContain('Signal détecté')
	})

	it('expands the detail of an event and marks reconstructed ones', () => {
		renderTimeline()
		const details = screen.getAllByText('Détails')
		expect(details).toHaveLength(2)
		fireEvent.click(details[1])
		expect(screen.getByText(/"value": "keycloak"/)).toBeTruthy()
		const summary = document.querySelector<HTMLElement>(
			'[data-event="summary"]',
		)
		expect(summary?.textContent).toContain('reconstitué')
		expect(summary?.querySelector('details')).toBeNull()
	})

	it('filters by step through its chips', () => {
		const { onToggleStep, onClearSteps } = renderTimeline({
			selected: ['qualification'],
		})
		const chip = screen.getByRole('button', { name: 'Qualification' })
		expect(chip.getAttribute('aria-pressed')).toBe('true')
		expect(
			screen
				.getByRole('button', { name: 'Toutes' })
				.getAttribute('aria-pressed'),
		).toBe('false')
		fireEvent.click(screen.getByRole('button', { name: 'Détection' }))
		expect(onToggleStep).toHaveBeenCalledWith('detection')
		fireEvent.click(screen.getByRole('button', { name: 'Toutes' }))
		expect(onClearSteps).toHaveBeenCalled()
	})

	it('lists the steps running now and the empty and error states', () => {
		renderTimeline({
			inFlight: [
				{
					step: 'inspection',
					actor: {
						module: 'dns_email',
						module_label: 'Écosystème DNS et e-mail',
					},
					started_at: '2026-10-09T09:59:00Z',
				},
			],
		})
		const running = document.querySelector<HTMLElement>('[data-in-flight]')
		expect(running?.textContent).toContain(
			'Inspection — Écosystème DNS et e-mail',
		)
		cleanup()
		renderTimeline({ rounds: [], steps: [] })
		expect(screen.getByText('Aucun événement enregistré.')).toBeTruthy()
		cleanup()
		renderTimeline({ state: 'error', errorMessage: 'boom', rounds: [] })
		expect(screen.getByRole('alert').textContent).toContain('boom')
	})
})
