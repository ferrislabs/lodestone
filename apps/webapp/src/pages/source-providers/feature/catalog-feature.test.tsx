import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { summary } from '#/pages/source-providers/lib/fixtures'
import { CatalogFeature } from './catalog-feature'

const router = vi.hoisted(() => ({ navigate: vi.fn() }))

vi.mock('@tanstack/react-router', () => ({
	useNavigate: () => router.navigate,
}))

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	router.navigate.mockReset()
	vi.stubGlobal(
		'fetch',
		vi.fn(
			async () =>
				new Response(
					JSON.stringify({
						data: [
							summary({ status: 'ready' }),
							summary({
								key: 'france_travail',
								name: 'France Travail',
								category: 'jobs_and_missions',
								auth_kind: 'oauth2_client_credentials',
							}),
						],
					}),
					{ status: 200, headers: { 'Content-Type': 'application/json' } },
				),
		),
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
			<CatalogFeature />
		</QueryClientProvider>,
	)
}

describe('CatalogFeature', () => {
	it('lists the providers from the API grouped by category', async () => {
		renderFeature()
		expect(await screen.findByText('BOAMP')).toBeTruthy()
		expect(
			screen.getByRole('region', { name: 'Offres et missions' }),
		).toBeTruthy()
		expect(screen.getByText('Prêt')).toBeTruthy()
		expect(screen.getByText('Non configuré')).toBeTruthy()
	})

	it('opens the page of a provider', async () => {
		renderFeature()
		fireEvent.click(
			await screen.findByRole('button', { name: /France Travail/ }),
		)
		expect(router.navigate).toHaveBeenCalledWith({
			to: '/sources/providers/$key',
			params: { key: 'france_travail' },
		})
	})

	it('keeps the run history reachable from the activity tab', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Activité' }))
		expect(router.navigate).toHaveBeenCalledWith({ to: '/sources/activity' })
	})
})
