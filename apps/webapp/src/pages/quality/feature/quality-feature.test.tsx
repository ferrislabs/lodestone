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

import type {
	Calibration,
	KeywordSuggestion,
	Quality,
} from '#/pages/quality/lib/types'
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
	exclusions: [],
	exclusion_candidates: [
		{
			lead_id: 'l1',
			domain: 'acme.test',
			legal_name: 'ACME SAS',
			reason: 'deja_client',
			decided_at: '2026-02-01T00:00:00Z',
		},
	],
	triage_recall: [],
	sourcing_yield: [
		{
			profile_id: 'p1',
			agent_id: 'offres-france',
			agent_name: 'Offres France',
			leads: 12,
			verdicts: { reliable: 5, to_verify: 4, discarded: 2, none: 1 },
			validated: 6,
			rejected: 3,
			precision: 0.67,
			keywords: [
				{
					keyword: 'cloud',
					leads: 8,
					exclusive_leads: 3,
					verdicts: { reliable: 3, to_verify: 2, discarded: 1, none: 2 },
					validated: 4,
					rejected: 1,
					precision: 0.8,
				},
			],
		},
	],
	keyword_suggestions: [],
}

const STAGE: KeywordSuggestion = {
	profile_id: 'p1',
	agent_id: 'offres-france',
	action: 'remove_keyword',
	word: 'stage',
	decided: 6,
	rejected: 6,
}

const ALTERNANCE: KeywordSuggestion = {
	profile_id: 'p1',
	agent_id: 'offres-france',
	action: 'add_exclusion',
	word: 'alternance',
	decided: 4,
	rejected: 4,
}

const CALIBRATION: Calibration = {
	required: 50,
	required_per_class: 10,
	profiles: [
		{
			profile_id: 'p1',
			name: 'Fintech',
			agents: [
				{
					kind: 'scoring',
					agent_id: 'score-global',
					name: 'Score global',
					n: 12,
					positives: 5,
					brier_before: 0.25,
					brier_after: null,
					status: 'insufficient',
					reliability: [],
					fitted_at: null,
				},
			],
		},
	],
}

let qualityCalls: URL[]
let calibrationCalls: string[]
let runResponse: { queued: boolean }
let calibrationStatus: number
let suggestions: KeywordSuggestion[]
let suggestionCalls: string[]
let applyBodies: unknown[]
let applyStatus: number
let profileStatus: number
let profileListCalls: number

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' },
	})
}

function install(status = 200) {
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string, init?: RequestInit) => {
			const url = new URL(String(input))
			if (url.pathname === '/api/v1/profiles/p1') {
				suggestionCalls.push('revision')
				return profileStatus === 200
					? json({ data: { id: 'p1', name: 'Fintech', revision: 3 } })
					: json(
							{ code: 'E_NOT_FOUND', message: 'Introuvable', status: 404 },
							profileStatus,
						)
			}
			if (url.pathname === '/api/v1/profiles/p1/keyword-suggestions/apply') {
				suggestionCalls.push('apply')
				applyBodies.push(JSON.parse(String(init?.body)))
				return applyStatus === 200
					? json({ data: { id: 'p1', name: 'Fintech', revision: 4 } })
					: json(
							{
								code: 'E_CONFLICT',
								message: 'Conflict: the profile moved to revision 5',
								status: applyStatus,
							},
							applyStatus,
						)
			}
			if (url.pathname === '/api/v1/quality') {
				qualityCalls.push(url)
				return status === 200
					? json({ data: { ...QUALITY, keyword_suggestions: suggestions } })
					: json({ code: 'E_INTERNAL', message: 'Boom', status }, status)
			}
			if (url.pathname === '/api/v1/quality/calibration/run') {
				calibrationCalls.push('run')
				return json({ data: runResponse }, 202)
			}
			if (url.pathname === '/api/v1/quality/calibration') {
				calibrationCalls.push('list')
				return calibrationStatus === 200
					? json({ data: CALIBRATION })
					: json(
							{ code: 'E_INTERNAL', message: 'Panne', status: 500 },
							calibrationStatus,
						)
			}
			profileListCalls += 1
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
	calibrationCalls = []
	runResponse = { queued: true }
	calibrationStatus = 200
	suggestions = [STAGE, ALTERNANCE]
	suggestionCalls = []
	applyBodies = []
	applyStatus = 200
	profileStatus = 200
	profileListCalls = 0
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

	it('copies the legal name and the domain of a rejected lead', async () => {
		const writeText = vi.fn().mockResolvedValue(undefined)
		vi.stubGlobal('navigator', { clipboard: { writeText } })
		renderFeature()
		fireEvent.click(
			await screen.findByRole('button', {
				name: "Copier ACME SAS pour la liste d'exclusion",
			}),
		)
		await waitFor(() => expect(screen.getByText('Copié')).toBeTruthy())
		expect(writeText).toHaveBeenCalledWith('ACME SAS, acme.test')
	})

	it('reports a refused clipboard', async () => {
		const writeText = vi.fn().mockRejectedValue(new Error('denied'))
		vi.stubGlobal('navigator', { clipboard: { writeText } })
		renderFeature()
		fireEvent.click(
			await screen.findByRole('button', {
				name: "Copier ACME SAS pour la liste d'exclusion",
			}),
		)
		await waitFor(() =>
			expect(screen.getByRole('alert').textContent).toContain('presse-papiers'),
		)
	})

	it('shows the calibration of each agent', async () => {
		renderFeature()
		const card = within(await screen.findByLabelText('Calibration'))
		expect(card.getByText('Pas assez de décisions (12/50)')).toBeTruthy()
		expect(card.getByText('Score global')).toBeTruthy()
	})

	it('queues a recalculation and reloads the calibration', async () => {
		renderFeature()
		const card = within(await screen.findByLabelText('Calibration'))
		fireEvent.click(card.getByRole('button', { name: 'Recalculer maintenant' }))
		await waitFor(() => expect(calibrationCalls).toContain('run'))
		await waitFor(() =>
			expect(screen.getByText(/Recalcul programmé : il démarre/)).toBeTruthy(),
		)
		await waitFor(() =>
			expect(
				calibrationCalls.filter((call) => call === 'list').length,
			).toBeGreaterThan(1),
		)
	})

	it('says when a recalculation is already waiting', async () => {
		runResponse = { queued: false }
		renderFeature()
		fireEvent.click(
			await screen.findByRole('button', { name: 'Recalculer maintenant' }),
		)
		await waitFor(() =>
			expect(screen.getByText('Un recalcul est déjà en attente.')).toBeTruthy(),
		)
	})

	it('reports a calibration that cannot be loaded without hiding the report', async () => {
		calibrationStatus = 500
		renderFeature()
		await screen.findByLabelText('Décisions')
		await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy())
		expect(screen.getByRole('alert').textContent).toContain('Panne')
		expect(screen.queryByLabelText('Calibration')).toBeNull()
	})

	it('reads the current revision then applies the suggestion and reloads the report', async () => {
		renderFeature()
		const line = (
			await screen.findByText(/Retirer le mot-clé « stage »/)
		).closest('li') as HTMLElement
		suggestions = [ALTERNANCE]
		fireEvent.click(within(line).getByRole('button', { name: 'Appliquer' }))
		await waitFor(() =>
			expect(within(line).getByText('Révision 4 créée')).toBeTruthy(),
		)
		expect(suggestionCalls).toEqual(['revision', 'apply'])
		expect(applyBodies).toEqual([
			{
				agent_id: 'offres-france',
				action: 'remove_keyword',
				word: 'stage',
				revision: 3,
			},
		])
		await waitFor(() => expect(qualityCalls.length).toBeGreaterThan(1))
		await waitFor(() => expect(profileListCalls).toBeGreaterThan(1))
		expect(within(line).queryByRole('button')).toBeNull()
		expect(screen.getByText(/Retirer le mot-clé « stage »/)).toBeTruthy()
		expect(
			within(
				screen.getByText(/Ajouter « alternance »/).closest('li') as HTMLElement,
			).getByRole('button', { name: 'Appliquer' }),
		).toBeTruthy()
	})

	it('applies each suggestion with its own body', async () => {
		renderFeature()
		const line = (await screen.findByText(/Ajouter « alternance »/)).closest(
			'li',
		) as HTMLElement
		fireEvent.click(within(line).getByRole('button', { name: 'Appliquer' }))
		await waitFor(() => expect(applyBodies).toHaveLength(1))
		expect(applyBodies[0]).toEqual({
			agent_id: 'offres-france',
			action: 'add_exclusion',
			word: 'alternance',
			revision: 3,
		})
	})

	it('shows the conflict message under the line and allows another try', async () => {
		applyStatus = 409
		renderFeature()
		const line = (
			await screen.findByText(/Retirer le mot-clé « stage »/)
		).closest('li') as HTMLElement
		fireEvent.click(within(line).getByRole('button', { name: 'Appliquer' }))
		await waitFor(() => expect(within(line).getByRole('alert')).toBeTruthy())
		expect(within(line).getByRole('alert').textContent).toBe(
			'the profile moved to revision 5',
		)
		expect(within(line).getByRole('button', { name: 'Appliquer' })).toBeTruthy()
	})

	it('shows the error and sends nothing when the profile cannot be read', async () => {
		profileStatus = 404
		renderFeature()
		const line = (
			await screen.findByText(/Retirer le mot-clé « stage »/)
		).closest('li') as HTMLElement
		fireEvent.click(within(line).getByRole('button', { name: 'Appliquer' }))
		await waitFor(() => expect(within(line).getByRole('alert')).toBeTruthy())
		expect(within(line).getByRole('alert').textContent).toBe('Introuvable')
		expect(suggestionCalls).toEqual(['revision'])
		expect(applyBodies).toEqual([])
	})

	it('opens the keywords of an agent', async () => {
		renderFeature()
		expect(screen.queryByRole('row', { name: /cloud/ })).toBeNull()
		const toggle = await screen.findByRole('button', {
			name: 'Mots-clés de Offres France',
		})
		fireEvent.click(toggle)
		expect(screen.getByRole('row', { name: /cloud/ })).toBeTruthy()
		expect(toggle.getAttribute('aria-expanded')).toBe('true')
		fireEvent.click(toggle)
		expect(screen.queryByRole('row', { name: /cloud/ })).toBeNull()
	})
})
