import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Trial, TrialDetail } from '#/pages/profiles/lib/types'
import { TrialsPanel } from '#/pages/profiles/ui/trials-panel'

afterEach(cleanup)

function trial(overrides: Partial<Trial> = {}): Trial {
	return {
		id: 't1',
		profile_id: 'p1',
		base_revision: 3,
		status: 'done',
		sample_size: 20,
		budget_cents: 200,
		spent_cents: 12.4,
		leads_tried: 18,
		created_at: '2026-10-20T10:00:00Z',
		...overrides,
	}
}

function detail(overrides: Partial<TrialDetail> = {}): TrialDetail {
	return {
		...trial(),
		summary: {
			current: {
				reliable: 4,
				reliable_validated: 2,
				precision: 0.5,
				rejected: 6,
				rejected_discarded: 3,
			},
			trial: {
				reliable: 3,
				reliable_validated: 3,
				precision: 1,
				rejected: 6,
				rejected_discarded: 5,
			},
			changed: 2,
			errors: 0,
		},
		leads: [
			{
				lead_id: 'l1',
				domain: 'acme.fr',
				label: 'validated',
				current_verdict: 'a_verifier',
				current_confidence: 55,
				trial_verdict: 'fiable',
				trial_confidence: 82,
			},
			{
				lead_id: 'l2',
				domain: 'same.fr',
				label: 'rejected',
				current_verdict: 'ecarte',
				current_confidence: 20,
				trial_verdict: 'ecarte',
				trial_confidence: 18,
			},
			{
				lead_id: 'l3',
				domain: 'broken.fr',
				label: 'rejected',
				current_verdict: 'fiable',
				current_confidence: 70,
				trial_verdict: null,
				trial_confidence: null,
				error: 'model call failed',
			},
		],
		...overrides,
	}
}

interface Overrides {
	trials?: readonly Trial[]
	selected?: string | null
	detail?: TrialDetail | null
	loading?: boolean
	error?: string | null
	loadingDetail?: boolean
	detailError?: string | null
	onSelect?: (id: string) => void
	onOpenLead?: (id: string) => void
}

function renderPanel(overrides: Overrides = {}) {
	return render(
		<TrialsPanel
			trials={overrides.trials ?? [trial()]}
			loading={overrides.loading ?? false}
			error={overrides.error ?? null}
			selected={overrides.selected ?? null}
			detail={overrides.detail ?? null}
			loadingDetail={overrides.loadingDetail ?? false}
			detailError={overrides.detailError ?? null}
			onSelect={overrides.onSelect ?? vi.fn()}
			onOpenLead={overrides.onOpenLead ?? vi.fn()}
		/>,
	)
}

function row(id: string): HTMLElement {
	const found = document.querySelector(`[data-trial="${id}"]`)
	if (!(found instanceof HTMLElement)) throw new Error(`no trial ${id}`)
	return found
}

function leadRow(id: string): HTMLElement {
	const found = document.querySelector(`[data-trial-lead="${id}"]`)
	if (!(found instanceof HTMLElement)) throw new Error(`no lead ${id}`)
	return found
}

describe('TrialsPanel list', () => {
	it('shows the revision, status, progress and cost of each trial', () => {
		renderPanel({
			trials: [
				trial(),
				trial({
					id: 't2',
					base_revision: 4,
					status: 'budget_exhausted',
					leads_tried: 7,
					spent_cents: 200,
				}),
			],
		})
		const first = within(row('t1'))
		expect(first.getByText('Révision 3 + brouillon')).toBeTruthy()
		expect(first.getByText('Terminé')).toBeTruthy()
		expect(first.getByText('18/20 leads')).toBeTruthy()
		expect(first.getByText('0,12 € sur 2,00 €')).toBeTruthy()
		const second = within(row('t2'))
		expect(second.getByText('Révision 4 + brouillon')).toBeTruthy()
		expect(second.getByText('Budget atteint')).toBeTruthy()
		expect(second.getByText('7/20 leads')).toBeTruthy()
		expect(second.getByText('2,00 € sur 2,00 €')).toBeTruthy()
	})

	it('names every status', () => {
		renderPanel({
			trials: [
				trial({ id: 'a', status: 'pending' }),
				trial({ id: 'b', status: 'running' }),
				trial({ id: 'c', status: 'failed' }),
			],
		})
		expect(within(row('a')).getByText('En attente')).toBeTruthy()
		expect(within(row('b')).getByText('En cours')).toBeTruthy()
		expect(within(row('c')).getByText('Échec')).toBeTruthy()
	})

	it('selects a trial on click and marks the selected one', () => {
		const onSelect = vi.fn()
		renderPanel({
			trials: [trial(), trial({ id: 't2' })],
			selected: 't2',
			onSelect,
		})
		fireEvent.click(within(row('t1')).getByRole('button'))
		expect(onSelect).toHaveBeenCalledWith('t1')
		expect(
			within(row('t2')).getByRole('button').getAttribute('aria-current'),
		).toBe('true')
		expect(
			within(row('t1')).getByRole('button').getAttribute('aria-current'),
		).toBe('false')
	})

	it('explains how to start a trial when there is none', () => {
		renderPanel({ trials: [] })
		expect(screen.getByText(/Aucun essai/)).toBeTruthy()
	})

	it('shows loading and error states', () => {
		const { unmount } = renderPanel({ trials: [], loading: true })
		expect(screen.getByText('Chargement des essais…')).toBeTruthy()
		unmount()
		renderPanel({ trials: [], error: 'Serveur injoignable' })
		expect(screen.getByRole('alert').textContent).toContain(
			'Serveur injoignable',
		)
	})

	it('asks to pick a trial before showing a detail', () => {
		renderPanel()
		expect(
			screen.getByText("Sélectionner un essai pour l'afficher."),
		).toBeTruthy()
	})
})

describe('TrialsPanel detail', () => {
	it('compares the current version with the draft', () => {
		renderPanel({ selected: 't1', detail: detail() })
		const summary = within(screen.getByRole('table', { name: 'Comparaison' }))
		expect(summary.getByText('Version actuelle')).toBeTruthy()
		expect(summary.getByText('Brouillon')).toBeTruthy()
		const reliable = within(
			summary.getByText('Fiable').closest('tr') as HTMLElement,
		)
		expect(reliable.getByText('4')).toBeTruthy()
		expect(reliable.getByText('3')).toBeTruthy()
		const validated = within(
			summary.getByText('dont validés').closest('tr') as HTMLElement,
		)
		expect(validated.getByText('2')).toBeTruthy()
		expect(validated.getByText('3')).toBeTruthy()
		const precision = within(
			summary.getByText('Précision').closest('tr') as HTMLElement,
		)
		expect(precision.getByText('50 %')).toBeTruthy()
		expect(precision.getByText('100 %')).toBeTruthy()
		const discarded = within(
			summary.getByText('Rejetés classés écartés').closest('tr') as HTMLElement,
		)
		expect(discarded.getByText('3/6')).toBeTruthy()
		expect(discarded.getByText('5/6')).toBeTruthy()
	})

	it('shows a dash when a version has no reliable lead', () => {
		const base = detail()
		renderPanel({
			selected: 't1',
			detail: detail({
				summary: {
					...base.summary,
					trial: { ...base.summary.trial, precision: null },
				},
			}),
		})
		const precision = within(
			screen.getByText('Précision').closest('tr') as HTMLElement,
		)
		expect(precision.getByText('—')).toBeTruthy()
	})

	it('counts the leads that change verdict and the errors', () => {
		renderPanel({
			selected: 't1',
			detail: detail({
				summary: { ...detail().summary, changed: 2, errors: 1 },
			}),
		})
		expect(screen.getByText('2 leads changent de verdict')).toBeTruthy()
		expect(screen.getByText('1 lead en erreur')).toBeTruthy()
	})

	it('hides the error count when nothing failed', () => {
		renderPanel({ selected: 't1', detail: detail() })
		expect(screen.queryByText(/en erreur/)).toBeNull()
	})

	it('marks the rows whose verdict changed', () => {
		renderPanel({ selected: 't1', detail: detail() })
		expect(leadRow('l1').hasAttribute('data-changed')).toBe(true)
		expect(leadRow('l2').hasAttribute('data-changed')).toBe(false)
		expect(leadRow('l3').hasAttribute('data-changed')).toBe(false)
	})

	it('lists each lead with its decision, verdicts and confidences', () => {
		renderPanel({ selected: 't1', detail: detail() })
		const first = within(leadRow('l1'))
		expect(first.getByText('acme.fr')).toBeTruthy()
		expect(first.getByText('Validé')).toBeTruthy()
		expect(first.getByText('À vérifier')).toBeTruthy()
		expect(first.getByText('Fiable')).toBeTruthy()
		expect(first.getByText('55 %')).toBeTruthy()
		expect(first.getByText('82 %')).toBeTruthy()
		expect(within(leadRow('l2')).getByText('Rejeté')).toBeTruthy()
	})

	it('shows the error of a lead that could not be tried', () => {
		renderPanel({ selected: 't1', detail: detail() })
		const broken = within(leadRow('l3'))
		expect(broken.getByText('model call failed')).toBeTruthy()
		expect(broken.queryAllByText('Fiable')).toHaveLength(1)
	})

	it('opens a lead from its domain', () => {
		const onOpenLead = vi.fn()
		renderPanel({ selected: 't1', detail: detail(), onOpenLead })
		const link = within(leadRow('l1')).getByRole('link', { name: 'acme.fr' })
		expect(link.getAttribute('href')).toBe('/leads/l1')
		fireEvent.click(link)
		expect(onOpenLead).toHaveBeenCalledWith('l1')
	})

	it('shows the error of a failed trial', () => {
		renderPanel({
			trials: [trial({ status: 'failed' })],
			selected: 't1',
			detail: detail({
				status: 'failed',
				error: 'worker crashed',
				leads: [],
			}),
		})
		expect(screen.getByRole('alert').textContent).toContain('worker crashed')
	})

	it('says a live trial is still being computed', () => {
		renderPanel({
			trials: [trial({ status: 'running' })],
			selected: 't1',
			detail: detail({ status: 'running', leads: [] }),
		})
		expect(
			screen.getByText(
				"L'essai est en cours, les résultats s'affichent au fur et à mesure.",
			),
		).toBeTruthy()
		expect(screen.queryByRole('table', { name: 'Comparaison' })).toBeNull()
	})

	it('shows loading and error states of the detail', () => {
		const { unmount } = renderPanel({ selected: 't1', loadingDetail: true })
		expect(screen.getByText("Chargement de l'essai…")).toBeTruthy()
		unmount()
		renderPanel({ selected: 't1', detailError: 'Ressource introuvable' })
		expect(screen.getByRole('alert').textContent).toContain(
			'Ressource introuvable',
		)
	})
})
