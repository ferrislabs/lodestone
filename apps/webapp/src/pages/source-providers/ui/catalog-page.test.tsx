import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { summary } from '#/pages/source-providers/lib/fixtures'
import { CatalogPage } from './catalog-page'

afterEach(cleanup)

const PROVIDERS = [
	summary(),
	summary({
		key: 'france_travail',
		name: 'France Travail',
		description: "Offres d'emploi.",
		category: 'jobs_and_missions',
		auth_kind: 'oauth2_client_credentials',
		status: 'failing',
	}),
	summary({
		key: 'ats',
		name: 'ATS',
		category: 'jobs_and_missions',
		auth_kind: 'api_key',
		status: 'untested',
	}),
	summary({
		key: 'ct',
		name: 'Certificats',
		category: 'technical_signals',
		status: 'ready',
		connected_account: {
			display_name: 'Alice Martin',
			scopes: [],
		},
	}),
]

function renderPage(
	overrides: Partial<Parameters<typeof CatalogPage>[0]> = {},
) {
	const props = {
		providers: PROVIDERS,
		state: 'ready' as const,
		errorMessage: null,
		onOpen: vi.fn(),
		onSelectTab: vi.fn(),
		...overrides,
	}
	render(<CatalogPage {...props} />)
	return props
}

describe('CatalogPage', () => {
	it('is titled the source catalog', () => {
		renderPage()
		expect(
			screen.getByRole('heading', { name: 'Catalogue des sources', level: 1 }),
		).toBeTruthy()
	})

	it('groups the cards by category', () => {
		renderPage()
		const procurement = screen.getByRole('region', { name: 'Marchés publics' })
		expect(within(procurement).getByText('BOAMP')).toBeTruthy()
		const jobs = screen.getByRole('region', { name: 'Offres et missions' })
		expect(within(jobs).getByText('France Travail')).toBeTruthy()
		expect(within(jobs).getByText('ATS')).toBeTruthy()
		expect(
			screen.getByRole('region', { name: 'Signaux techniques' }),
		).toBeTruthy()
		expect(screen.queryByRole('region', { name: 'Entreprises' })).toBeNull()
	})

	it('shows a status badge, the auth kind and the description on each card', () => {
		renderPage()
		const card = screen.getByRole('button', { name: /France Travail/ })
		expect(within(card).getByText('En échec')).toBeTruthy()
		expect(within(card).getByText('OAuth2 · compte de service')).toBeTruthy()
		expect(within(card).getByText("Offres d'emploi.")).toBeTruthy()
		expect(screen.getByText('Non configuré')).toBeTruthy()
		expect(screen.getByText('À tester')).toBeTruthy()
		expect(screen.getByText('Prêt')).toBeTruthy()
		expect(screen.getByText('Compte : Alice Martin')).toBeTruthy()
	})

	it('opens a provider when its card is clicked', () => {
		const props = renderPage()
		fireEvent.click(screen.getByRole('button', { name: /France Travail/ }))
		expect(props.onOpen).toHaveBeenCalledWith('france_travail')
	})

	it('offers the activity tab', () => {
		const props = renderPage()
		expect(
			screen
				.getByRole('button', { name: 'Catalogue' })
				.getAttribute('aria-current'),
		).toBe('page')
		fireEvent.click(screen.getByRole('button', { name: 'Activité' }))
		expect(props.onSelectTab).toHaveBeenCalledWith('activity')
	})

	it('shows an error and no card when loading fails', () => {
		renderPage({ state: 'error', errorMessage: 'Serveur injoignable' })
		expect(screen.getByRole('alert').textContent).toBe('Serveur injoignable')
		expect(screen.queryByText('BOAMP')).toBeNull()
	})
})
