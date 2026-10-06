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

import type { Quality } from '#/pages/quality/lib/types'
import { QualityFeature } from './quality-feature'

const router = vi.hoisted(() => ({
	search: {} as { period?: string; profile?: string },
	navigate: vi.fn(),
}))

vi.mock('@tanstack/react-router', () => ({
	useSearch: () => router.search,
	useNavigate: () => router.navigate,
}))

const QUALITY: Quality = {
	decisions: {
		validated: 7,
		rejected: 3,
		drafts_approved: 1,
		drafts_rejected: 0,
	},
	verdicts: [
		{ verdict: 'fiable', validated: 7, rejected: 3, n: 10, precision: 0.7 },
	],
	discarded: { discarded: 1, total: 4, share: 0.25 },
	confidence: Array.from({ length: 10 }, (_, index) => ({
		from: index * 10,
		to: index * 10 + 10,
		validated: 0,
		rejected: 0,
	})),
	agents: [],
	versions: [],
	profiles: [
		{
			profile_id: 'p1',
			name: 'Fintech',
			validated: 7,
			rejected: 3,
			precision: 0.7,
			current: { reliable_from: 70, discard_below: 30 },
			suggestion: {
				ready: false,
				labelled: 10,
				required: 20,
				reliable_from: null,
				discard_below: null,
			},
			rejection_reasons: [],
		},
	],
}

let qualityCalls: URL[]

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' },
	})
}

function install(status = 200) {
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string) => {
			const url = new URL(String(input))
			if (url.pathname === '/api/v1/quality') {
				qualityCalls.push(url)
				return status === 200
					? json({ data: QUALITY })
					: json({ code: 'E_INTERNAL', message: 'Boom', status }, status)
			}
			return json({
				data: [{ id: 'p1', name: 'Fintech', lead_count: 3, active: true }],
			})
		}),
	)
}

function renderFeature() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	})
	render(
		<QueryClientProvider client={client}>
			<QualityFeature />
		</QueryClientProvider>,
	)
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	qualityCalls = []
	router.search = {}
	router.navigate.mockReset()
	install()
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
})

describe('QualityFeature', () => {
	it('loads the last thirty days of every profile by default', async () => {
		renderFeature()
		const card = within(await screen.findByLabelText('Décisions'))
		expect(card.getByText('Leads validés').nextSibling?.textContent).toBe('7')
		expect(qualityCalls).toHaveLength(1)
		const params = qualityCalls[0].searchParams
		expect(params.get('profile_id')).toBeNull()
		const from = Date.parse(params.get('from') ?? '')
		const thirtyDays = 30 * 86_400_000
		expect(Math.abs(Date.now() - from - thirtyDays)).toBeLessThan(60_000)
		expect(
			within(screen.getByLabelText('Suggestions pour Fintech')).getByText(
				'Pas assez de décisions (10/20)',
			),
		).toBeTruthy()
	})

	it('sends no lower bound for the whole history and the chosen profile', async () => {
		router.search = { period: 'all', profile: 'p1' }
		renderFeature()
		await screen.findByLabelText('Décisions')
		const params = qualityCalls[0].searchParams
		expect(params.get('from')).toBeNull()
		expect(params.get('profile_id')).toBe('p1')
	})

	it('puts the period and the profile in the address', async () => {
		renderFeature()
		await screen.findByLabelText('Décisions')

		fireEvent.click(screen.getByLabelText('7 j'))
		expect(router.navigate).toHaveBeenLastCalledWith({
			to: '/quality',
			search: { period: '7' },
		})

		fireEvent.change(screen.getByLabelText('Profil'), {
			target: { value: 'p1' },
		})
		expect(router.navigate).toHaveBeenLastCalledWith({
			to: '/quality',
			search: { profile: 'p1' },
		})
	})

	it('opens the Qualification tab of a profile', async () => {
		renderFeature()
		const section = await screen.findByLabelText('Suggestions pour Fintech')
		fireEvent.click(
			within(section).getByRole('button', {
				name: "Ouvrir l'onglet Qualification",
			}),
		)
		expect(router.navigate).toHaveBeenCalledWith({
			to: '/profiles/$profileId',
			params: { profileId: 'p1' },
			search: { tab: 'qualification' },
		})
	})

	it('shows the error when the report cannot be loaded', async () => {
		install(500)
		renderFeature()
		await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy())
		expect(screen.getByRole('alert').textContent).toContain('Boom')
	})
})
