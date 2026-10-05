import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Source } from '#/pages/sources/lib/types'
import { SourcesPage } from './sources-page'

afterEach(cleanup)

const NOW = Date.parse('2026-10-05T12:00:00Z')

function source(name: string, overrides: Partial<Source> = {}): Source {
	return {
		source: name,
		kind: 'source.run',
		label_key: `source.${name}`,
		status: 'pending',
		attempts: 0,
		next_run_at: '2026-10-05T14:00:00Z',
		last_runs: [],
		...overrides,
	}
}

const SOURCES: Source[] = [
	source('boamp', {
		last_runs: [
			{
				profile_id: 'p1',
				profile_name: 'Fintech',
				status: 'succeeded',
				volume: 12,
				signals: 3,
				started_at: '2026-10-05T11:50:00Z',
				finished_at: '2026-10-05T11:55:00Z',
			},
		],
	}),
	source('decp', {
		status: 'running',
		running_since: '2026-10-05T11:58:00Z',
		next_run_at: '2026-10-05T11:57:00Z',
	}),
	source('ted', {
		status: 'failed',
		last_runs: [
			{
				profile_id: 'p1',
				profile_name: 'Fintech',
				status: 'failed',
				volume: 0,
				signals: 0,
				error: 'upstream error: timeout',
				started_at: '2026-10-05T11:00:00Z',
				finished_at: '2026-10-05T11:01:00Z',
			},
		],
	}),
	source('france_travail', {
		status: 'unavailable',
		unavailable_reason: 'missing_credentials',
		next_run_at: null,
	}),
]

function renderPage(
	overrides: Partial<Parameters<typeof SourcesPage>[0]> = {},
) {
	const props = {
		sources: SOURCES,
		state: 'ready' as const,
		errorMessage: null,
		now: NOW,
		scheduled: new Set<string>(),
		pending: new Set<string>(),
		errors: {},
		onRun: vi.fn(),
		...overrides,
	}
	render(<SourcesPage {...props} />)
	return props
}

function row(label: string) {
	return within(screen.getByRole('row', { name: new RegExp(label) }))
}

describe('SourcesPage', () => {
	it('groups the sources by family', () => {
		renderPage()
		expect(screen.getByRole('region', { name: 'Marchés publics' })).toBeTruthy()
		expect(screen.getByRole('region', { name: "Offres d'emploi" })).toBeTruthy()
	})

	it('shows a state badge for each state', () => {
		renderPage()
		expect(row('BOAMP').getByText('En attente')).toBeTruthy()
		expect(row('DECP').getByText('En cours')).toBeTruthy()
		expect(row('TED').getAllByText('Échec').length).toBe(2)
		expect(row('France Travail').getByText('Indisponible')).toBeTruthy()
	})

	it('shows the next run relative and absolute', () => {
		renderPage()
		expect(row('BOAMP').getByText('dans 2 heures')).toBeTruthy()
		expect(row('BOAMP').getByText(/5 oct\. 2026/)).toBeTruthy()
	})

	it('summarises the last run with counts and a short error', () => {
		renderPage()
		expect(row('BOAMP').getByText('12 éléments, 3 signaux')).toBeTruthy()
		expect(row('BOAMP').getByText('Fintech')).toBeTruthy()
		expect(row('TED').getByText('upstream error: timeout')).toBeTruthy()
		expect(row('DECP').getByText('Aucun passage')).toBeTruthy()
	})

	it('enables the button only for a pending source', () => {
		renderPage()
		const enabled = row('BOAMP').getByRole('button', {
			name: 'Lancer maintenant',
		})
		expect((enabled as HTMLButtonElement).disabled).toBe(false)
		for (const label of ['DECP', 'TED', 'France Travail']) {
			const button = row(label).getByRole('button', {
				name: 'Lancer maintenant',
			})
			expect((button as HTMLButtonElement).disabled).toBe(true)
		}
	})

	it('explains why a button is disabled', () => {
		renderPage()
		expect(row('DECP').getByText('La source est déjà en cours')).toBeTruthy()
		expect(
			row('France Travail').getByText(
				'Identifiants manquants : la source est désactivée',
			),
		).toBeTruthy()
	})

	it('reports the click with the source name', () => {
		const props = renderPage()
		fireEvent.click(
			row('BOAMP').getByRole('button', { name: 'Lancer maintenant' }),
		)
		expect(props.onRun).toHaveBeenCalledWith('boamp')
	})

	it('shows the scheduled and pending labels', () => {
		renderPage({
			scheduled: new Set(['boamp']),
			pending: new Set(['ted']),
		})
		const scheduled = row('BOAMP').getByRole('button', {
			name: 'Planifié maintenant',
		})
		expect((scheduled as HTMLButtonElement).disabled).toBe(true)
		expect(
			row('TED').getByRole('button', { name: 'Planification…' }),
		).toBeTruthy()
	})

	it('shows an action error next to the button', () => {
		renderPage({ errors: { boamp: 'La source est déjà en cours' } })
		expect(row('BOAMP').getByRole('alert').textContent).toBe(
			'La source est déjà en cours',
		)
	})

	it('shows a loading and an error state', () => {
		renderPage({
			state: 'error',
			errorMessage: 'Serveur injoignable',
			sources: [],
		})
		expect(screen.getByRole('alert').textContent).toBe('Serveur injoignable')
	})
})
