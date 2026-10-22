import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from '@testing-library/react'
import {
	afterEach,
	beforeAll,
	beforeEach,
	describe,
	expect,
	it,
	vi,
} from 'vitest'

import { newAgent } from '#/pages/profiles/lib/agents'
import { newQualificationAgent } from '#/pages/profiles/lib/qualification'
import type { Trial, TrialDetail } from '#/pages/profiles/lib/types'
import { ProfileDetailFeature } from './profile-detail-feature'

const router = vi.hoisted(() => {
	let search: Record<string, unknown> = {}
	const listeners = new Set<() => void>()
	return {
		get: () => search,
		set: (next: Record<string, unknown>) => {
			search = next
			for (const listener of listeners) listener()
		},
		subscribe: (listener: () => void) => {
			listeners.add(listener)
			return () => {
				listeners.delete(listener)
			}
		},
	}
})

vi.mock('@tanstack/react-router', async () => {
	const { useSyncExternalStore } = await import('react')
	return {
		useParams: () => ({ profileId: 'p1' }),
		useSearch: () => useSyncExternalStore(router.subscribe, router.get),
		useNavigate: () => (options: { search?: Record<string, unknown> }) =>
			router.set(options.search ?? {}),
	}
})

class NoopMediaQuery {
	matches = false
	addEventListener() {}
	removeEventListener() {}
}

class NoopResizeObserver {
	observe() {}
	unobserve() {}
	disconnect() {}
}

beforeAll(() => {
	vi.stubGlobal('ResizeObserver', NoopResizeObserver)
	vi.stubGlobal('matchMedia', () => new NoopMediaQuery())
})

const DEFINITION = {
	schema_version: 14,
	identity: { offer: 'Gestion des identités', target_customer: 'ETI' },
	agents: [newAgent('search', [], ['boamp'])],
	qualification_agents: [newQualificationAgent('pertinence', new Set())],
	aggregator: { provider_id: null, system_prompt: 'p' },
	verdict: { reliable_from: 70, discard_below: 30 },
	contact_ranking: { preferred_local_parts: [] },
	language: 'fr',
}

const PROFILE = {
	id: 'p1',
	name: 'Fintech',
	active: true,
	revision: 3,
	definition: DEFINITION,
	created_at: '2026-01-01T00:00:00Z',
	updated_at: '2026-02-01T00:00:00Z',
}

function trial(overrides: Partial<Trial> = {}): Trial {
	return {
		id: 't-new',
		profile_id: 'p1',
		base_revision: 3,
		status: 'pending',
		sample_size: 10,
		budget_cents: 150,
		spent_cents: 0,
		leads_tried: 0,
		created_at: '2026-10-20T10:00:00Z',
		...overrides,
	}
}

function trialDetail(overrides: Partial<TrialDetail> = {}): TrialDetail {
	return {
		...trial(),
		summary: {
			current: {
				reliable: 1,
				reliable_validated: 1,
				precision: 1,
				rejected: 1,
				rejected_discarded: 0,
			},
			trial: {
				reliable: 2,
				reliable_validated: 1,
				precision: 0.5,
				rejected: 1,
				rejected_discarded: 1,
			},
			changed: 1,
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
		],
		...overrides,
	}
}

interface Call {
	method: string
	path: string
	body: unknown
}

let calls: Call[]
let startResponse: () => Response
let trials: Trial[]
let detailStatus: string

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
			const method = init?.method ?? 'GET'
			const body = typeof init?.body === 'string' ? JSON.parse(init.body) : null
			calls.push({ method, path: url.pathname, body })
			if (method === 'POST' && url.pathname === '/api/v1/profiles/p1/trials') {
				return startResponse()
			}
			if (method === 'PUT') return json({ data: PROFILE })
			if (url.pathname === '/api/v1/profiles/p1') return json({ data: PROFILE })
			if (url.pathname === '/api/v1/profiles/p1/trials') {
				return json({ data: trials })
			}
			if (url.pathname.startsWith('/api/v1/trials/')) {
				const id = url.pathname.split('/').pop()
				return json({
					data: trialDetail({
						id,
						status: detailStatus,
						leads: detailStatus === 'pending' ? [] : trialDetail().leads,
					}),
				})
			}
			if (url.pathname === '/api/v1/quality/calibration') {
				return json({ data: { profiles: [], required: 10 } })
			}
			return json({ data: [] })
		}),
	)
}

function renderFeature() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	})
	render(
		<QueryClientProvider client={client}>
			<ProfileDetailFeature />
		</QueryClientProvider>,
	)
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	calls = []
	trials = []
	detailStatus = 'pending'
	router.set({})
	startResponse = () => json({ data: trial() }, 202)
	install()
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
	vi.useRealTimers()
})

function posts(): Call[] {
	return calls.filter((call) => call.method === 'POST')
}

function puts(): Call[] {
	return calls.filter((call) => call.method === 'PUT')
}

function detailCalls(id: string): Call[] {
	return calls.filter(
		(call) => call.method === 'GET' && call.path === `/api/v1/trials/${id}`,
	)
}

async function editOffer(value: string) {
	fireEvent.change(await screen.findByLabelText(/^Offre/), {
		target: { value },
	})
}

function openLauncher() {
	fireEvent.click(
		screen.getByRole('button', { name: 'Essayer sur les leads décidés' }),
	)
	return within(
		screen.getByRole('group', { name: 'Essai sur les leads décidés' }),
	)
}

describe('ProfileDetailFeature trial launcher', () => {
	it('offers the trial only once the draft differs from the revision', async () => {
		renderFeature()
		await screen.findByLabelText(/^Offre/)
		expect(
			screen.queryByRole('button', { name: 'Essayer sur les leads décidés' }),
		).toBeNull()
		await editOffer('Autre offre')
		expect(
			screen.getByRole('button', { name: 'Essayer sur les leads décidés' }),
		).toBeTruthy()
	})

	it('asks for the sample size and the budget with their defaults', async () => {
		renderFeature()
		await editOffer('Autre offre')
		const launcher = openLauncher()
		expect(
			(launcher.getByLabelText("Taille de l'échantillon") as HTMLInputElement)
				.value,
		).toBe('20')
		expect(
			(launcher.getByLabelText('Budget (centimes)') as HTMLInputElement).value,
		).toBe('200')
		expect(
			launcher.getByText(
				"Les agents du brouillon sont lancés sur des leads déjà validés ou rejetés. Rien n'est écrit sur ces leads ; les appels comptent dans le budget.",
			),
		).toBeTruthy()
		expect(posts()).toHaveLength(0)
	})

	it('posts the unsaved draft with the chosen limits and never saves it', async () => {
		renderFeature()
		await editOffer('Autre offre')
		const launcher = openLauncher()
		fireEvent.change(launcher.getByLabelText("Taille de l'échantillon"), {
			target: { value: '10' },
		})
		fireEvent.change(launcher.getByLabelText('Budget (centimes)'), {
			target: { value: '150' },
		})
		fireEvent.click(launcher.getByRole('button', { name: "Lancer l'essai" }))
		await waitFor(() => expect(posts()).toHaveLength(1))
		expect(posts()[0].path).toBe('/api/v1/profiles/p1/trials')
		expect(posts()[0].body).toEqual({
			definition: {
				...DEFINITION,
				identity: { ...DEFINITION.identity, offer: 'Autre offre' },
			},
			sample_size: 10,
			budget_cents: 150,
		})
		expect(puts()).toHaveLength(0)
	})

	it('does not send a request when a limit is out of range', async () => {
		renderFeature()
		await editOffer('Autre offre')
		const launcher = openLauncher()
		fireEvent.change(launcher.getByLabelText("Taille de l'échantillon"), {
			target: { value: '51' },
		})
		fireEvent.click(launcher.getByRole('button', { name: "Lancer l'essai" }))
		expect(
			await screen.findByText(
				"La taille de l'échantillon doit être un entier de 1 à 50",
			),
		).toBeTruthy()
		expect(posts()).toHaveLength(0)
	})

	it('shows the problems of an invalid draft without opening the form', async () => {
		renderFeature()
		await editOffer('')
		fireEvent.click(
			screen.getByRole('button', { name: 'Essayer sur les leads décidés' }),
		)
		expect(
			screen.queryByRole('group', { name: 'Essai sur les leads décidés' }),
		).toBeNull()
		expect(
			await screen.findAllByText('Ne doit pas être vide'),
		).not.toHaveLength(0)
		expect(posts()).toHaveLength(0)
	})

	it('shows the problems the server finds in the definition', async () => {
		startResponse = () =>
			json(
				{
					code: 'E_UNPROCESSABLE_ENTITY',
					message: 'Request payload rejected',
					status: 422,
					details: [{ path: 'identity.offer', message: 'is too long' }],
				},
				422,
			)
		renderFeature()
		await editOffer('Autre offre')
		const launcher = openLauncher()
		fireEvent.click(launcher.getByRole('button', { name: "Lancer l'essai" }))
		expect(await screen.findAllByText('Trop long')).not.toHaveLength(0)
		expect(router.get()).toEqual({})
		expect(puts()).toHaveLength(0)
	})

	it('shows a conflict next to the button and keeps the form open', async () => {
		startResponse = () =>
			json(
				{
					code: 'E_CONFLICT',
					message:
						'Conflict: a trial is already pending or running for this profile',
					status: 409,
				},
				409,
			)
		renderFeature()
		await editOffer('Autre offre')
		const launcher = openLauncher()
		fireEvent.click(launcher.getByRole('button', { name: "Lancer l'essai" }))
		expect(
			await launcher.findByText(
				'Un essai est déjà en attente ou en cours pour ce profil',
			),
		).toBeTruthy()
		expect(router.get()).toEqual({})
	})

	it('explains when no lead has been decided yet', async () => {
		startResponse = () =>
			json(
				{
					code: 'E_CONFLICT',
					message: 'Conflict: no decided lead to try',
					status: 409,
				},
				409,
			)
		renderFeature()
		await editOffer('Autre offre')
		const launcher = openLauncher()
		fireEvent.click(launcher.getByRole('button', { name: "Lancer l'essai" }))
		expect(
			await launcher.findByText(
				"Aucun lead décidé à essayer : validez ou rejetez d'abord quelques leads",
			),
		).toBeTruthy()
	})

	it('closes the form on cancel without sending anything', async () => {
		renderFeature()
		await editOffer('Autre offre')
		const launcher = openLauncher()
		fireEvent.click(launcher.getByRole('button', { name: 'Annuler' }))
		expect(
			screen.queryByRole('group', { name: 'Essai sur les leads décidés' }),
		).toBeNull()
		expect(posts()).toHaveLength(0)
	})
})

describe('ProfileDetailFeature trials tab', () => {
	it('opens the trials tab on the new trial and polls until it is done', async () => {
		vi.useFakeTimers({ shouldAdvanceTime: true })
		trials = [trial()]
		renderFeature()
		await editOffer('Autre offre')
		const launcher = openLauncher()
		fireEvent.click(launcher.getByRole('button', { name: "Lancer l'essai" }))
		await waitFor(() =>
			expect(router.get()).toEqual({ tab: 'trials', trial: 't-new' }),
		)
		expect(await screen.findAllByText('Révision 3 + brouillon')).toHaveLength(2)
		await waitFor(() => expect(detailCalls('t-new')).toHaveLength(1))
		expect(
			await screen.findByText(
				"L'essai est en cours, les résultats s'affichent au fur et à mesure.",
			),
		).toBeTruthy()

		detailStatus = 'running'
		await vi.advanceTimersByTimeAsync(3_100)
		await waitFor(() => expect(detailCalls('t-new')).toHaveLength(2))

		detailStatus = 'done'
		trials = [trial({ status: 'done', leads_tried: 10, spent_cents: 90 })]
		await vi.advanceTimersByTimeAsync(3_100)
		const table = await screen.findByRole('table', { name: 'Comparaison' })
		expect(within(table).getByText('Version actuelle')).toBeTruthy()
		expect(screen.getByText('1 lead change de verdict')).toBeTruthy()
		expect(screen.getByText('acme.fr')).toBeTruthy()

		const settled = detailCalls('t-new').length
		await vi.advanceTimersByTimeAsync(10_000)
		expect(detailCalls('t-new')).toHaveLength(settled)
	})

	it('lists the trials of the profile and opens one from the list', async () => {
		trials = [
			trial({ id: 't1', status: 'done', base_revision: 2 }),
			trial({ id: 't2', status: 'failed', base_revision: 3 }),
		]
		detailStatus = 'done'
		router.set({ tab: 'trials' })
		renderFeature()
		expect(await screen.findByText('Révision 2 + brouillon')).toBeTruthy()
		expect(screen.getByText('Échec')).toBeTruthy()
		expect(detailCalls('t1')).toHaveLength(0)
		fireEvent.click(
			within(
				screen.getByText('Révision 2 + brouillon').closest('li') as HTMLElement,
			).getByRole('button'),
		)
		await waitFor(() =>
			expect(router.get()).toEqual({ tab: 'trials', trial: 't1' }),
		)
		expect(
			await screen.findByRole('table', { name: 'Comparaison' }),
		).toBeTruthy()
		expect(detailCalls('t1').length).toBeGreaterThan(0)
	})

	it('keeps the selected trial in the address and reads it back', async () => {
		trials = [trial({ id: 't1', status: 'done' })]
		detailStatus = 'done'
		router.set({ tab: 'trials', trial: 't1' })
		renderFeature()
		expect(
			await screen.findByRole('table', { name: 'Comparaison' }),
		).toBeTruthy()
		expect(detailCalls('t1').length).toBeGreaterThan(0)
	})
})
