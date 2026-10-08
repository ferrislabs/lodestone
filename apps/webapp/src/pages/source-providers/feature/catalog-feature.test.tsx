import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { summary } from '#/pages/source-providers/lib/fixtures'
import { CatalogFeature } from './catalog-feature'

const router = vi.hoisted(() => ({
	navigate: vi.fn(),
	search: {} as { category?: string; q?: string },
}))

vi.mock('@tanstack/react-router', () => ({
	useNavigate: () => router.navigate,
	useSearch: () => router.search,
}))

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	router.navigate.mockReset()
	router.search = {}
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
	it('lists the providers from the API ', async () => {
		renderFeature()
		expect(await screen.findByText('BOAMP')).toBeTruthy()
		expect(screen.getByText('France Travail')).toBeTruthy()
	})

	it('opens the page of a provider', async () => {
		renderFeature()
		fireEvent.click(
			await screen.findByRole('link', { name: /^France Travail/ }),
		)
		expect(router.navigate).toHaveBeenCalledWith({
			to: '/sources/providers/$key',
			params: { key: 'france_travail' },
		})
	})

	it('applies the filters from the route search params', async () => {
		router.search = { category: 'jobs_and_missions', q: 'travail' }
		renderFeature()
		expect(await screen.findByText('France Travail')).toBeTruthy()
		expect(screen.queryByText('BOAMP')).toBeNull()
		expect((screen.getByRole('searchbox') as HTMLInputElement).value).toBe(
			'travail',
		)
	})

	it('writes the filters to the route search params', async () => {
		router.search = { q: 'bo' }
		renderFeature()
		await screen.findByText('BOAMP')
		fireEvent.click(screen.getByRole('button', { name: /^Offres et missions/ }))
		expect(router.navigate).toHaveBeenCalledWith({
			to: '/sources',
			search: { q: 'bo', status: undefined, category: 'jobs_and_missions' },
			replace: true,
		})
		fireEvent.change(screen.getByRole('searchbox'), {
			target: { value: '' },
		})
		expect(router.navigate).toHaveBeenLastCalledWith({
			to: '/sources',
			search: { category: undefined, status: undefined, q: undefined },
			replace: true,
		})
	})

	it('keeps the run history reachable from the activity tab', async () => {
		renderFeature()
		fireEvent.click(await screen.findByRole('button', { name: 'Activité' }))
		expect(router.navigate).toHaveBeenCalledWith({ to: '/sources/activity' })
	})

	it('opens the collection rules panel from the header', async () => {
		renderFeature()
		fireEvent.click(
			await screen.findByRole('button', { name: 'Règles de collecte' }),
		)
		expect(
			await screen.findByRole('dialog', { name: 'Règles de collecte' }),
		).toBeTruthy()
		expect(
			vi
				.mocked(fetch)
				.mock.calls.some(([url]) =>
					String(url).endsWith('/api/v1/settings/crawl'),
				),
		).toBe(true)
	})
})
