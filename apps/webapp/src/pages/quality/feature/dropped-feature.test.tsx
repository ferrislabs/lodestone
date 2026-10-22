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

import type { DroppedSearch } from '#/pages/quality/lib/dropped'
import type { DroppedCandidate } from '#/pages/quality/lib/types'
import { DroppedFeature } from './dropped-feature'

const router = vi.hoisted(() => ({
	search: {} as DroppedSearch,
	navigate: vi.fn(),
}))

vi.mock('@tanstack/react-router', () => ({
	useSearch: () => router.search,
	useNavigate: () => router.navigate,
}))

const FIRST: DroppedCandidate = {
	id: 'd1',
	profile_id: 'p1',
	agent: 'offres',
	source: 'france_travail',
	kind: 'job_offer',
	domain: 'acme.test',
	title: 'Développeur Rust',
	excerpts: ['Nous cherchons un profil backend.'],
	provenance: 'https://jobs.acme.test/rust',
	observed_at: '2026-10-01T08:00:00Z',
	reason: 'exclusion',
	matched: ['rust'],
	excluded: ['stage'],
	dropped_at: '2026-10-02T09:30:00Z',
	review: null,
	reviewed_at: null,
}

const SECOND: DroppedCandidate = {
	...FIRST,
	id: 'd2',
	title: 'Comptable',
	excerpts: [],
	matched: [],
	excluded: [],
	reason: 'no_keyword',
}

const THIRD: DroppedCandidate = { ...FIRST, id: 'd3', title: 'Data engineer' }

let sample: DroppedCandidate[][]
let droppedCalls: URL[]
let reviewCalls: { id: string; body: unknown }[]
let reviewResponse: () => Response

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' },
	})
}

function install() {
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string, init?: RequestInit) => {
			const url = new URL(String(input))
			if (url.pathname === '/api/v1/profiles/p1/dropped') {
				droppedCalls.push(url)
				return json({
					data: sample[Math.min(droppedCalls.length, sample.length) - 1],
				})
			}
			const review = /^\/api\/v1\/dropped\/(.+)\/review$/.exec(url.pathname)
			if (review !== null) {
				reviewCalls.push({
					id: review[1],
					body: JSON.parse(String(init?.body)),
				})
				return reviewResponse()
			}
			return json({
				data: [
					{ id: 'p1', name: 'Fintech', lead_count: 3, active: true },
					{ id: 'p2', name: 'Santé', lead_count: 1, active: true },
				],
			})
		}),
	)
}

function renderFeature() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	})
	const invalidate = vi.spyOn(client, 'invalidateQueries')
	render(
		<QueryClientProvider client={client}>
			<DroppedFeature />
		</QueryClientProvider>,
	)
	return { invalidate }
}

function card(id: string) {
	const element = document.querySelector(`[data-candidate="${id}"]`)
	if (element === null) throw new Error(`no card for ${id}`)
	return within(element as HTMLElement)
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	sample = [[FIRST, SECOND]]
	droppedCalls = []
	reviewCalls = []
	reviewResponse = () =>
		json({
			data: {
				candidate: {
					...FIRST,
					review: 'should_have_kept',
					reviewed_at: '2026-10-03T10:00:00Z',
				},
				lead_id: 'l9',
			},
		})
	router.search = { profile: 'p1' }
	router.navigate.mockReset()
	install()
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
})

describe('DroppedFeature', () => {
	it('asks for a profile and does not call the API without one', async () => {
		router.search = {}
		renderFeature()
		expect(await screen.findByText(/Choisissez un profil/)).toBeTruthy()
		await screen.findByRole('option', { name: 'Fintech' })
		expect(droppedCalls).toHaveLength(0)
	})

	it('loads a random sample of the unreviewed candidates of the profile', async () => {
		renderFeature()
		expect(await screen.findByText('Développeur Rust')).toBeTruthy()
		expect(screen.getByText('Comptable')).toBeTruthy()
		expect(droppedCalls).toHaveLength(1)
		const params = droppedCalls[0].searchParams
		expect(params.get('reviewed')).toBe('false')
		expect(params.get('order')).toBe('random')
		expect(params.get('limit')).toBe('20')
		expect(params.get('reason')).toBeNull()
		expect(params.get('agent')).toBeNull()
	})

	it('filters by reason, agent and review state', async () => {
		router.search = {
			profile: 'p1',
			reason: 'filter_country',
			agent: 'offres',
			reviewed: 'done',
		}
		renderFeature()
		await screen.findByText('Développeur Rust')
		const params = droppedCalls[0].searchParams
		expect(params.get('reason')).toBe('filter_country')
		expect(params.get('agent')).toBe('offres')
		expect(params.get('reviewed')).toBe('true')
	})

	it('sends no reviewed filter for every candidate', async () => {
		router.search = { profile: 'p1', reviewed: 'all' }
		renderFeature()
		await screen.findByText('Développeur Rust')
		expect(droppedCalls[0].searchParams.get('reviewed')).toBeNull()
	})

	it('puts the filters in the address', async () => {
		router.search = { profile: 'p1', reason: 'exclusion', agent: 'offres' }
		renderFeature()
		await screen.findByText('Développeur Rust')

		fireEvent.change(screen.getByLabelText('Profil'), {
			target: { value: 'p2' },
		})
		expect(router.navigate).toHaveBeenLastCalledWith({
			to: '/quality/dropped',
			search: { profile: 'p2', reason: 'exclusion' },
		})

		fireEvent.change(screen.getByLabelText('Raison'), {
			target: { value: 'filter_remote' },
		})
		expect(router.navigate).toHaveBeenLastCalledWith({
			to: '/quality/dropped',
			search: { profile: 'p1', reason: 'filter_remote', agent: 'offres' },
		})

		fireEvent.click(screen.getByLabelText('Toutes'))
		expect(router.navigate).toHaveBeenLastCalledWith({
			to: '/quality/dropped',
			search: {
				profile: 'p1',
				reason: 'exclusion',
				agent: 'offres',
				reviewed: 'all',
			},
		})

		fireEvent.click(
			screen.getByRole('button', { name: /Retirer le filtre agent/ }),
		)
		expect(router.navigate).toHaveBeenLastCalledWith({
			to: '/quality/dropped',
			search: { profile: 'p1', reason: 'exclusion' },
		})
	})

	it('reviews a candidate, keeps it in place and does not reshuffle the sample', async () => {
		const { invalidate } = renderFeature()
		await screen.findByText('Développeur Rust')
		fireEvent.click(
			card('d1').getByRole('button', { name: 'Aurait dû être gardée' }),
		)
		await waitFor(() =>
			expect(
				card('d1').getByRole('link', { name: 'Voir le lead' }),
			).toBeTruthy(),
		)
		expect(reviewCalls).toEqual([
			{ id: 'd1', body: { review: 'should_have_kept' } },
		])
		expect(card('d1').getByText('Aurait dû être gardée')).toBeTruthy()
		expect(
			card('d1').queryByRole('button', { name: 'Bien rejetée' }),
		).toBeNull()
		expect(
			card('d2').getByRole('button', { name: 'Bien rejetée' }),
		).toBeTruthy()
		expect(droppedCalls).toHaveLength(1)
		expect(invalidate).toHaveBeenCalledWith({
			queryKey: [{ _id: '/api/v1/quality' }],
		})
	})

	it('opens the lead created by a review', async () => {
		renderFeature()
		await screen.findByText('Développeur Rust')
		fireEvent.click(
			card('d1').getByRole('button', { name: 'Aurait dû être gardée' }),
		)
		fireEvent.click(
			await card('d1').findByRole('link', { name: 'Voir le lead' }),
		)
		expect(router.navigate).toHaveBeenCalledWith({
			to: '/leads/$leadId',
			params: { leadId: 'l9' },
		})
	})

	it('records a rightly dropped review without a lead', async () => {
		reviewResponse = () =>
			json({
				data: {
					candidate: {
						...FIRST,
						review: 'rightly_dropped',
						reviewed_at: '2026-10-03T10:00:00Z',
					},
					lead_id: null,
				},
			})
		renderFeature()
		await screen.findByText('Développeur Rust')
		fireEvent.click(card('d1').getByRole('button', { name: 'Bien rejetée' }))
		await waitFor(() =>
			expect(
				card('d1').queryByRole('button', { name: 'Bien rejetée' }),
			).toBeNull(),
		)
		expect(card('d1').getByText('Bien rejetée')).toBeTruthy()
		expect(card('d1').queryByRole('link', { name: 'Voir le lead' })).toBeNull()
		expect(reviewCalls[0].body).toEqual({ review: 'rightly_dropped' })
	})

	it('shows the error of a refused review on its card', async () => {
		reviewResponse = () =>
			json(
				{
					code: 'E_CONFLICT',
					message: 'Cette annonce a déjà été revue',
					status: 409,
				},
				409,
			)
		renderFeature()
		await screen.findByText('Développeur Rust')
		fireEvent.click(card('d1').getByRole('button', { name: 'Bien rejetée' }))
		await waitFor(() => expect(card('d1').getByRole('alert')).toBeTruthy())
		expect(card('d1').getByRole('alert').textContent).toBe(
			'Cette annonce a déjà été revue',
		)
		expect(card('d2').queryByRole('alert')).toBeNull()
		expect(
			card('d1').getByRole('button', { name: 'Bien rejetée' }),
		).toBeTruthy()
	})

	it('draws a new sample on demand', async () => {
		sample = [[FIRST, SECOND], [THIRD]]
		renderFeature()
		await screen.findByText('Développeur Rust')
		fireEvent.click(screen.getByRole('button', { name: 'Nouvel échantillon' }))
		expect(await screen.findByText('Data engineer')).toBeTruthy()
		expect(screen.queryByText('Développeur Rust')).toBeNull()
		expect(droppedCalls).toHaveLength(2)
	})

	it('says so when the sample is empty', async () => {
		sample = [[]]
		renderFeature()
		expect(
			await screen.findByText('Aucune annonce écartée pour ces critères.'),
		).toBeTruthy()
	})
})
