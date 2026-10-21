import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { detail } from '#/pages/source-providers/lib/fixtures'
import type { ProviderDetail } from '#/pages/source-providers/lib/types'
import { ProviderFeature } from './provider-feature'

const router = vi.hoisted(() => ({
	navigate: vi.fn(),
	search: {} as { connected?: boolean; error?: string },
}))

vi.mock('@tanstack/react-router', () => ({
	useParams: () => ({ key: 'france_travail' }),
	useSearch: () => router.search,
	useNavigate: () => router.navigate,
}))

interface Call {
	method: string
	path: string
	body: unknown
}

let calls: Call[]
let current: ProviderDetail
let putStatus: number
let assign: ReturnType<typeof vi.fn>

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'Content-Type': 'application/json' },
	})
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	calls = []
	current = detail()
	putStatus = 200
	router.navigate.mockReset()
	router.search = {}
	assign = vi.fn()
	Object.defineProperty(window, 'location', {
		configurable: true,
		value: { assign },
	})
	vi.stubGlobal(
		'fetch',
		vi.fn(async (input: URL | string, init?: RequestInit) => {
			const url = new URL(String(input))
			const method = init?.method ?? 'GET'
			const body = typeof init?.body === 'string' ? JSON.parse(init.body) : null
			calls.push({ method, path: url.pathname, body })
			if (method === 'PUT' && putStatus === 422) {
				return json(
					{
						code: 'E_UNPROCESSABLE_ENTITY',
						message: 'Request payload rejected',
						status: 422,
						details: [{ path: 'client_id', message: 'is required' }],
					},
					422,
				)
			}
			if (url.pathname.endsWith('/oauth/start')) {
				return json({
					data: { authorize_url: 'https://auth.example.com/authorize?state=s' },
				})
			}
			if (method === 'DELETE') return new Response(null, { status: 204 })
			return json({ data: current })
		}),
	)
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
})

function renderFeature() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	})
	render(
		<QueryClientProvider client={client}>
			<ProviderFeature />
		</QueryClientProvider>,
	)
}

function writes(): Call[] {
	return calls.filter((call) => call.method !== 'GET')
}

describe('ProviderFeature', () => {
	it('loads the provider and shows its secret masked', async () => {
		renderFeature()
		expect(await screen.findByText('France Travail')).toBeTruthy()
		expect(
			(screen.getByLabelText('Clé secrète (masqué)') as HTMLInputElement).value,
		).toBe('••••abcd')
		expect(calls[0]?.path).toBe('/api/v1/source-providers/france_travail')
	})

	it('saves the settings without the masked secret', async () => {
		renderFeature()
		const input = (await screen.findByLabelText(
			'Identifiant client *',
		)) as HTMLInputElement
		fireEvent.change(input, { target: { value: 'client-2' } })
		fireEvent.click(
			screen.getByRole('button', { name: 'Enregistrer et tester' }),
		)
		await waitFor(() => expect(writes()).toHaveLength(1))
		expect(writes()[0]).toEqual({
			method: 'PUT',
			path: '/api/v1/source-providers/france_travail',
			body: {
				settings: {
					client_id: 'client-2',
					api_base_url: 'https://api.example.com/v2',
				},
				secrets: {},
			},
		})
	})

	it('sends a replacement secret only after the operator chose to replace it', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Remplacer' }))
		fireEvent.change(screen.getByLabelText('Clé secrète *'), {
			target: { value: 'brand-new-secret' },
		})
		fireEvent.click(
			screen.getByRole('button', { name: 'Enregistrer et tester' }),
		)
		await waitFor(() => expect(writes()).toHaveLength(1))
		expect(writes()[0]?.body).toMatchObject({
			secrets: { client_secret: 'brand-new-secret' },
		})
	})

	it('shows the server validation errors on their fields', async () => {
		putStatus = 422
		renderFeature()
		fireEvent.change(await screen.findByLabelText('Identifiant client *'), {
			target: { value: 'x' },
		})
		fireEvent.click(
			screen.getByRole('button', { name: 'Enregistrer et tester' }),
		)
		expect(await screen.findByText('Champ obligatoire')).toBeTruthy()
	})

	it('tests the connection on demand', async () => {
		renderFeature()
		fireEvent.click(
			await screen.findByRole('button', { name: 'Tester la connexion' }),
		)
		await waitFor(() => expect(writes()).toHaveLength(1))
		expect(writes()[0]?.path).toBe(
			'/api/v1/source-providers/france_travail/test',
		)
	})

	it('redirects the browser to the provider to connect an account', async () => {
		current = detail({}, { auth_kind: 'oauth2_authorization_code' })
		renderFeature()
		fireEvent.click(
			await screen.findByRole('button', { name: 'Connecter un compte' }),
		)
		await waitFor(() =>
			expect(assign).toHaveBeenCalledWith(
				'https://auth.example.com/authorize?state=s',
			),
		)
		expect(writes()[0]?.path).toBe(
			'/api/v1/source-providers/france_travail/oauth/start',
		)
	})

	it('disconnects the account', async () => {
		current = detail(
			{},
			{
				auth_kind: 'oauth2_authorization_code',
				connected_account: { display_name: 'Alice Martin', scopes: [] },
			},
		)
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Déconnecter' }))
		await waitFor(() => expect(writes()).toHaveLength(1))
		expect(writes()[0]?.path).toBe(
			'/api/v1/source-providers/france_travail/disconnect',
		)
	})

	it('announces a connected account after the OAuth redirect', async () => {
		router.search = { connected: true }
		renderFeature()
		expect(
			await screen.findByText('Compte connecté. La connexion a été testée.'),
		).toBeTruthy()
	})

	it('explains an OAuth error after the redirect', async () => {
		router.search = { error: 'expired_state' }
		renderFeature()
		expect(await screen.findByText(/a expiré/)).toBeTruthy()
	})

	it('deletes the configuration after a confirmation and returns to the catalog', async () => {
		renderFeature()
		fireEvent.click(
			await screen.findByRole('button', { name: 'Supprimer la configuration' }),
		)
		expect(writes()).toHaveLength(0)
		fireEvent.click(
			screen.getByRole('button', { name: 'Confirmer la suppression' }),
		)
		await waitFor(() => expect(writes()).toHaveLength(1))
		expect(writes()[0]?.method).toBe('DELETE')
		await waitFor(() =>
			expect(router.navigate).toHaveBeenCalledWith({ to: '/sources' }),
		)
	})
})
