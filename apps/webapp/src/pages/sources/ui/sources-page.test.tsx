import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { formatDateTime } from '#/lib/format'
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
		mode: 'scheduled',
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
		unavailable_reason: 'not_scheduled',
		next_run_at: null,
	}),
]

function listening(
	name: string,
	listener: Partial<NonNullable<Source['listener']>> = {},
): Source {
	return source(name, {
		mode: 'continuous',
		status: listener.state ?? 'listening',
		next_run_at: null,
		listener: {
			state: 'listening',
			holder: 'node-a-1',
			last_activity_at: '2026-10-05T11:57:00Z',
			items_total: 1200,
			signals_total: 8,
			backlog: 12,
			...listener,
		},
	})
}

const LISTENERS: Source[] = [
	listening('certificate_transparency', { backlog: 42 }),
	listening('dns_cname', { backlog: 1 }),
	listening('http_probe', {
		state: 'standby',
		holder: null,
		last_activity_at: null,
		backlog: null,
	}),
	listening('security_txt', { state: 'idle_no_agent', backlog: 0 }),
	listening('public_page', {
		state: 'error',
		last_error: 'upstream error: dns down',
		last_error_at: '2026-10-05T11:58:00Z',
		backlog: 3,
	}),
	listening('role_guess', {
		state: 'idle',
		backlog: 5,
		last_error: 'stale failure',
		last_error_at: '2026-10-05T11:00:00Z',
	}),
	listening('qualification', { backlog: 7 }),
]

const STALE_LISTENER = listening('dns_cname', {
	state: 'stale',
	updated_at: '2026-10-05T11:50:00Z',
})

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
		onOpenProvider: vi.fn(),
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
				"Aucune tâche n'est planifiée pour cette source",
			),
		).toBeTruthy()
	})

	it('flags a source whose provider is not ready with a link to its catalog page', () => {
		const props = renderPage({
			sources: [
				source('boamp', {
					provider: { key: 'boamp', name: 'BOAMP', status: 'failing' },
				}),
				source('decp', {
					provider: { key: 'decp', name: 'DECP', status: 'ready' },
				}),
			],
		})
		expect(row('BOAMP').getByText('Provider non prêt')).toBeTruthy()
		expect(row('DECP').queryByText('Provider non prêt')).toBeNull()
		fireEvent.click(
			row('BOAMP').getByRole('button', {
				name: 'Ouvrir BOAMP dans le catalogue',
			}),
		)
		expect(props.onOpenProvider).toHaveBeenCalledWith('boamp')
		const run = row('BOAMP').getByRole('button', { name: 'Lancer maintenant' })
		expect((run as HTMLButtonElement).disabled).toBe(true)
		expect(
			row('BOAMP').getByText('Le fournisseur doit être prêt dans le catalogue'),
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

	describe('continuous listeners', () => {
		function renderListeners() {
			return renderPage({ sources: [...LISTENERS, ...SOURCES] })
		}

		it('shows them in their own group before the scheduled ones', () => {
			renderListeners()
			const regions = screen.getAllByRole('region')
			expect(regions[0]?.getAttribute('aria-label')).toBe('Écoute continue')
			const group = within(
				screen.getByRole('region', { name: 'Écoute continue' }),
			)
			expect(group.getAllByRole('row').length).toBe(1 + LISTENERS.length)
			expect(group.queryByRole('button')).toBeNull()
		})

		it('labels the listener states', () => {
			renderListeners()
			expect(
				row('Transparence des certificats').getByText('En écoute'),
			).toBeTruthy()
			expect(
				row('Sonde HTTP').getByText('En veille (autre instance)'),
			).toBeTruthy()
			expect(
				row('security\\.txt').getByText('Inactif (aucun agent)'),
			).toBeTruthy()
			expect(row('Page publique').getByText('Erreur')).toBeTruthy()
			expect(
				row('Page publique').getByText('upstream error: dns down'),
			).toBeTruthy()
			expect(
				row('Page publique').getByText(
					`(${formatDateTime('2026-10-05T11:58:00Z')})`,
				),
			).toBeTruthy()
			expect(row('Adresse de rôle devinée').getByText('En écoute')).toBeTruthy()
			expect(
				row('Adresse de rôle devinée').queryByText('stale failure'),
			).toBeNull()
		})

		it('shows the last activity relative to now', () => {
			renderListeners()
			expect(
				row('Transparence des certificats').getByText('il y a 3 minutes'),
			).toBeTruthy()
			expect(row('Sonde HTTP').getByText('Aucune activité')).toBeTruthy()
		})

		it('labels the backlog per source', () => {
			renderListeners()
			expect(
				row('Transparence des certificats').getByText(
					'En attente : 42 tuiles de retard',
				),
			).toBeTruthy()
			expect(
				row('DNS CNAME').getByText('En attente : 1 hôte à sonder'),
			).toBeTruthy()
			expect(
				row('Page publique').getByText('En attente : 3 cibles de contact'),
			).toBeTruthy()
			expect(row('Sonde HTTP').getByText('—')).toBeTruthy()
			expect(
				row('Qualification et synthèse des leads').getByText(
					'En attente : 7 leads à qualifier',
				),
			).toBeTruthy()
		})

		it('flags a silent listener with its last update and an explanation', () => {
			renderPage({ sources: [STALE_LISTENER] })
			const dns = row('DNS CNAME')
			expect(dns.getByText('Sans nouvelles')).toBeTruthy()
			expect(
				dns.getByText(
					/dernière mise à jour il y a 10 minutes\. L'instance qui écoutait ne répond plus ; une autre prendra le relais\./,
				),
			).toBeTruthy()
		})

		it('flags a listener whose provider is not ready', () => {
			const props = renderPage({
				sources: [
					listening('dns_cname', {
						state: 'idle',
					}),
				].map((listener) => ({
					...listener,
					provider: {
						key: 'dns_cname',
						name: 'DNS CNAME',
						status: 'not_configured',
					},
				})),
			})
			expect(row('DNS CNAME').getByText('Provider non prêt')).toBeTruthy()
			fireEvent.click(
				row('DNS CNAME').getByRole('button', {
					name: 'Ouvrir DNS CNAME dans le catalogue',
				}),
			)
			expect(props.onOpenProvider).toHaveBeenCalledWith('dns_cname')
		})

		it('shows the totals', () => {
			renderListeners()
			expect(
				row('Transparence des certificats').getByText(
					/1\s200 éléments traités, 8 signaux/,
				),
			).toBeTruthy()
		})
	})
})
