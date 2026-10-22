import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { DroppedEntry } from '#/pages/quality/lib/dropped'
import type { DroppedCandidate } from '#/pages/quality/lib/types'
import { DroppedPage } from './dropped-page'

afterEach(cleanup)

const CANDIDATE: DroppedCandidate = {
	id: 'd1',
	profile_id: 'p1',
	agent: 'offres',
	source: 'france_travail',
	kind: 'job_offer',
	domain: 'acme.test',
	title: 'Développeur Rust',
	excerpts: ['Nous cherchons un profil backend.', 'Stack : Rust et Postgres.'],
	provenance: 'https://jobs.acme.test/rust',
	observed_at: '2026-10-01T08:00:00Z',
	reason: 'exclusion',
	matched: ['rust', 'postgres'],
	excluded: ['stage'],
	dropped_at: '2026-10-02T09:30:00Z',
	review: null,
	reviewed_at: null,
}

const SECOND: DroppedCandidate = {
	...CANDIDATE,
	id: 'd2',
	agent: null,
	source: 'ats',
	domain: 'globex.test',
	title: 'Comptable',
	excerpts: [],
	provenance: 'https://globex.test/jobs/1',
	reason: 'no_keyword',
	matched: [],
	excluded: [],
}

function entry(
	candidate: DroppedCandidate,
	overrides: Partial<DroppedEntry> = {},
): DroppedEntry {
	return { candidate, leadId: null, error: null, pending: false, ...overrides }
}

function renderPage(
	overrides: Partial<Parameters<typeof DroppedPage>[0]> = {},
) {
	const props = {
		profiles: [
			{ id: 'p1', name: 'Fintech', lead_count: 3, active: true },
			{ id: 'p2', name: 'Santé', lead_count: 1, active: true },
		],
		profileId: 'p1',
		onProfileChange: vi.fn(),
		reason: '' as const,
		onReasonChange: vi.fn(),
		agent: null,
		onClearAgent: vi.fn(),
		reviewState: 'todo' as const,
		onReviewStateChange: vi.fn(),
		state: 'ready' as const,
		errorMessage: null,
		entries: [entry(CANDIDATE), entry(SECOND)],
		refreshing: false,
		onRefresh: vi.fn(),
		onReview: vi.fn(),
		onOpenLead: vi.fn(),
		...overrides,
	}
	render(<DroppedPage {...props} />)
	return props
}

function card(id: string) {
	const element = document.querySelector(`[data-candidate="${id}"]`)
	if (element === null) throw new Error(`no card for ${id}`)
	return within(element as HTMLElement)
}

describe('DroppedPage', () => {
	it('asks for a profile and lists nothing without one', () => {
		renderPage({ profileId: '', entries: [] })
		expect(screen.getByText(/Choisissez un profil/)).toBeTruthy()
		expect(screen.queryByText(/Aucune annonce écartée/)).toBeNull()
		expect(screen.queryByRole('list', { name: 'Annonces écartées' })).toBeNull()
	})

	it('shows each candidate with its context, excerpts and chips', () => {
		renderPage()
		const first = card('d1')
		expect(first.getByText('Développeur Rust')).toBeTruthy()
		expect(first.getByText(/acme\.test/)).toBeTruthy()
		expect(first.getByText(/offres/)).toBeTruthy()
		expect(first.getByText('Mot exclu')).toBeTruthy()
		expect(first.getByText('Mots-clés trouvés')).toBeTruthy()
		expect(first.getByText('rust')).toBeTruthy()
		expect(first.getByText('postgres')).toBeTruthy()
		expect(first.getByText('Mots exclus')).toBeTruthy()
		expect(first.getByText('stage')).toBeTruthy()
		const quotes = document.querySelectorAll('[data-candidate="d1"] blockquote')
		expect(Array.from(quotes).map((quote) => quote.textContent)).toEqual([
			'Nous cherchons un profil backend.',
			'Stack : Rust et Postgres.',
		])
		const link = first.getByRole('link', { name: "Voir l'annonce" })
		expect(link.getAttribute('href')).toBe('https://jobs.acme.test/rust')
		expect(link.getAttribute('target')).toBe('_blank')
		expect(link.getAttribute('rel')).toContain('noreferrer')
	})

	it('shows the company job offers without an agent and hides empty chip lists', () => {
		renderPage()
		const second = card('d2')
		expect(second.getByText(/Offres d'emploi de l'entreprise/)).toBeTruthy()
		expect(second.getByText('Aucun mot-clé')).toBeTruthy()
		expect(second.queryByText('Mots-clés trouvés')).toBeNull()
		expect(second.queryByText('Mots exclus')).toBeNull()
		expect(
			document.querySelectorAll('[data-candidate="d2"] blockquote'),
		).toHaveLength(0)
	})

	it('does not link an announcement that is not a web page', () => {
		renderPage({
			entries: [entry({ ...CANDIDATE, provenance: 'javascript:alert(1)' })],
		})
		expect(
			card('d1').queryByRole('link', { name: "Voir l'annonce" }),
		).toBeNull()
	})

	it('reviews a candidate with each button', () => {
		const props = renderPage()
		const first = card('d1')
		fireEvent.click(first.getByRole('button', { name: 'Bien rejetée' }))
		expect(props.onReview).toHaveBeenLastCalledWith(
			CANDIDATE,
			'rightly_dropped',
		)
		fireEvent.click(
			first.getByRole('button', { name: 'Aurait dû être gardée' }),
		)
		expect(props.onReview).toHaveBeenLastCalledWith(
			CANDIDATE,
			'should_have_kept',
		)
	})

	it('disables the buttons while a review is pending', () => {
		renderPage({ entries: [entry(CANDIDATE, { pending: true })] })
		const first = card('d1')
		expect(
			first
				.getByRole('button', { name: 'Bien rejetée' })
				.hasAttribute('disabled'),
		).toBe(true)
		expect(
			first
				.getByRole('button', { name: 'Aurait dû être gardée' })
				.hasAttribute('disabled'),
		).toBe(true)
	})

	it('shows the error of a failed review on its card only', () => {
		renderPage({
			entries: [
				entry(CANDIDATE, { error: 'Cette annonce a déjà été revue' }),
				entry(SECOND),
			],
		})
		expect(card('d1').getByRole('alert').textContent).toBe(
			'Cette annonce a déjà été revue',
		)
		expect(card('d2').queryByRole('alert')).toBeNull()
		expect(
			card('d1').getByRole('button', { name: 'Bien rejetée' }),
		).toBeTruthy()
	})

	it('keeps a reviewed card in place with its review and the lead', () => {
		const props = renderPage({
			entries: [
				entry(
					{
						...CANDIDATE,
						review: 'should_have_kept',
						reviewed_at: '2026-10-03T10:00:00Z',
					},
					{ leadId: 'l9' },
				),
				entry(SECOND),
			],
		})
		const first = card('d1')
		expect(first.getByText('Aurait dû être gardée')).toBeTruthy()
		expect(first.queryByRole('button', { name: 'Bien rejetée' })).toBeNull()
		expect(
			first.queryByRole('button', { name: 'Aurait dû être gardée' }),
		).toBeNull()
		const lead = first.getByRole('link', { name: 'Voir le lead' })
		expect(lead.getAttribute('href')).toBe('/leads/l9')
		fireEvent.click(lead)
		expect(props.onOpenLead).toHaveBeenCalledWith('l9')
		expect(
			card('d2').getByRole('button', { name: 'Bien rejetée' }),
		).toBeTruthy()
	})

	it('shows a rightly dropped review without a lead link', () => {
		renderPage({
			entries: [
				entry({
					...CANDIDATE,
					review: 'rightly_dropped',
					reviewed_at: '2026-10-03T10:00:00Z',
				}),
			],
		})
		const first = card('d1')
		expect(first.getByText('Bien rejetée')).toBeTruthy()
		expect(first.queryByRole('link', { name: 'Voir le lead' })).toBeNull()
	})

	it('says so when nothing was dropped for the criteria', () => {
		renderPage({ entries: [] })
		expect(
			screen.getByText('Aucune annonce écartée pour ces critères.'),
		).toBeTruthy()
	})

	it('shows a loading state and an error without candidates', () => {
		renderPage({ state: 'loading', entries: [] })
		expect(screen.queryByText(/Aucune annonce écartée/)).toBeNull()
		cleanup()
		renderPage({ state: 'error', errorMessage: 'Panne', entries: [] })
		expect(screen.getByRole('alert').textContent).toBe('Panne')
	})

	it('changes the profile, the reason and the state', () => {
		const props = renderPage()
		fireEvent.change(screen.getByLabelText('Profil'), {
			target: { value: 'p2' },
		})
		expect(props.onProfileChange).toHaveBeenCalledWith('p2')
		fireEvent.change(screen.getByLabelText('Raison'), {
			target: { value: 'filter_country' },
		})
		expect(props.onReasonChange).toHaveBeenCalledWith('filter_country')
		fireEvent.change(screen.getByLabelText('Raison'), { target: { value: '' } })
		expect(props.onReasonChange).toHaveBeenLastCalledWith('')
		fireEvent.click(screen.getByLabelText('Revues'))
		expect(props.onReviewStateChange).toHaveBeenCalledWith('done')
		expect(
			(screen.getByLabelText('À revoir') as HTMLInputElement).checked,
		).toBe(true)
	})

	it('offers every reason and the whole set of reasons', () => {
		renderPage()
		const options = within(screen.getByLabelText('Raison'))
			.getAllByRole('option')
			.map((option) => option.textContent)
		expect(options).toEqual([
			'Toutes les raisons',
			'Aucun mot-clé',
			'Mot exclu',
			'Filtre télétravail',
			'Filtre pays',
			'Filtre contrat',
		])
	})

	it('draws a new sample', () => {
		const props = renderPage()
		fireEvent.click(screen.getByRole('button', { name: 'Nouvel échantillon' }))
		expect(props.onRefresh).toHaveBeenCalledTimes(1)
		cleanup()
		renderPage({ refreshing: true })
		expect(
			screen
				.getByRole('button', { name: 'Nouvel échantillon' })
				.hasAttribute('disabled'),
		).toBe(true)
	})

	it('shows the agent filter and clears it', () => {
		const props = renderPage({ agent: 'offres' })
		fireEvent.click(
			screen.getByRole('button', { name: /Retirer le filtre agent/ }),
		)
		expect(props.onClearAgent).toHaveBeenCalledTimes(1)
	})
})
