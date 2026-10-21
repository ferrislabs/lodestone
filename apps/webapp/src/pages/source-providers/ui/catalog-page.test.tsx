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
		category: null,
		query: '',
		status: null,
		onStatusChange: vi.fn(),
		onCategoryChange: vi.fn(),
		onQueryChange: vi.fn(),
		onReset: vi.fn(),
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

	it('renders one grid of all the cards that fills the row', () => {
		renderPage()
		const grids = screen.getAllByTestId('provider-grid')
		expect(grids).toHaveLength(1)
		expect(grids[0].className).toContain(
			'grid-cols-[repeat(auto-fill,minmax(220px,1fr))]',
		)
		expect(screen.getAllByTestId('provider-card')).toHaveLength(4)
		expect(screen.queryByRole('region')).toBeNull()
	})

	it('shows category chips with counts and marks the selected one', () => {
		renderPage({ category: 'jobs_and_missions' })
		const group = screen.getByRole('group', { name: 'Filtrer par type' })
		expect(within(group).getByRole('button', { name: 'Tous 4' })).toBeTruthy()
		expect(
			within(group)
				.getByRole('button', { name: 'Offres et missions 2' })
				.getAttribute('aria-pressed'),
		).toBe('true')
		expect(
			within(group)
				.getByRole('button', { name: 'Marchés publics 1' })
				.getAttribute('aria-pressed'),
		).toBe('false')
		expect(
			within(group).queryByRole('button', { name: /Entreprises/ }),
		).toBeNull()
		expect(screen.getAllByTestId('provider-card')).toHaveLength(2)
	})

	it('toggles a category chip and resets with Tous', () => {
		const props = renderPage({ category: 'jobs_and_missions' })
		const group = screen.getByRole('group', { name: 'Filtrer par type' })
		fireEvent.click(
			within(group).getByRole('button', { name: 'Offres et missions 2' }),
		)
		expect(props.onCategoryChange).toHaveBeenLastCalledWith(null)
		fireEvent.click(
			within(group).getByRole('button', { name: 'Marchés publics 1' }),
		)
		expect(props.onCategoryChange).toHaveBeenLastCalledWith(
			'public_procurement',
		)
		fireEvent.click(within(group).getByRole('button', { name: 'Tous 4' }))
		expect(props.onCategoryChange).toHaveBeenLastCalledWith(null)
	})

	it('filters by category when a card badge is clicked without opening it', () => {
		const props = renderPage()
		const card = screen
			.getAllByTestId('provider-card')
			.find((node) => within(node).queryByText('BOAMP')) as HTMLElement
		fireEvent.click(
			within(card).getByRole('button', { name: 'Filtrer par Marchés publics' }),
		)
		expect(props.onCategoryChange).toHaveBeenCalledWith('public_procurement')
		expect(props.onOpen).not.toHaveBeenCalled()
	})

	it('searches without accents or case and reports the typed text', () => {
		const props = renderPage({ query: 'CERTIFICAT' })
		expect(screen.getAllByTestId('provider-card')).toHaveLength(1)
		expect(screen.getByText('Certificats')).toBeTruthy()
		fireEvent.change(screen.getByPlaceholderText('Rechercher une source…'), {
			target: { value: 'ats' },
		})
		expect(props.onQueryChange).toHaveBeenCalledWith('ats')
	})

	it('shows an empty state with a reset button', () => {
		const props = renderPage({ query: 'zzzz' })
		expect(screen.getByText('Aucune source ne correspond.')).toBeTruthy()
		expect(screen.queryByTestId('provider-grid')).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: /Réinitialiser/ }))
		expect(props.onReset).toHaveBeenCalled()
	})

	it('shows the auth kind and description but no status badge', () => {
		renderPage()
		const card = screen
			.getByRole('link', { name: /^France Travail/ })
			.closest('[data-testid="provider-card"]') as HTMLElement
		expect(within(card).queryByText('En échec')).toBeNull()
		expect(within(card).getByText('OAuth2 · compte de service')).toBeTruthy()
		expect(within(card).getByText("Offres d'emploi.")).toBeTruthy()
		expect(card.getAttribute('title')).toBe('France Travail — En échec')
		expect(screen.getByText('Compte : Alice Martin')).toBeTruthy()
	})

	it('colours the border by status', () => {
		renderPage()
		const border = (name: RegExp) =>
			(
				screen
					.getByRole('link', { name })
					.closest('[data-testid="provider-card"]') as HTMLElement
			).className
		expect(border(/^BOAMP/)).toContain('border-dashed')
		expect(border(/^France Travail/)).toContain('border-red-500')
		expect(border(/^ATS/)).toContain('border-amber-500')
		expect(border(/^Certificats/)).toContain('border-emerald-500')
	})

	it('shows a status legend that toggles the status filter', () => {
		const props = renderPage({ status: 'failing' })
		const legend = screen.getByRole('group', { name: 'Filtrer par statut' })
		for (const label of ['Prêt', 'À tester', 'En échec', 'Non configuré']) {
			expect(within(legend).getByRole('button', { name: label })).toBeTruthy()
		}
		expect(
			within(legend)
				.getByRole('button', { name: 'En échec' })
				.getAttribute('aria-pressed'),
		).toBe('true')
		expect(screen.getAllByTestId('provider-card')).toHaveLength(1)
		fireEvent.click(within(legend).getByRole('button', { name: 'En échec' }))
		expect(props.onStatusChange).toHaveBeenLastCalledWith(null)
		fireEvent.click(within(legend).getByRole('button', { name: 'Prêt' }))
		expect(props.onStatusChange).toHaveBeenLastCalledWith('ready')
		fireEvent.click(screen.getByRole('button', { name: 'Tous 4' }))
		expect(props.onStatusChange).toHaveBeenLastCalledWith(null)
	})

	it('renders square cards that link to the provider page', () => {
		renderPage()
		const link = screen.getByRole('link', { name: /^France Travail/ })
		const card = link.closest('[data-testid="provider-card"]') as HTMLElement
		expect(card.className).toContain('aspect-square')
		expect(card.className).toContain('has-[a:focus-visible]:ring-')
		expect(link.className).toContain('after:absolute')
		expect(link.getAttribute('aria-label')).toBe('France Travail — En échec')
		expect(link.getAttribute('href')).toBe('/sources/providers/france_travail')
	})

	it('shows the real logo with the provider name as alt text', () => {
		renderPage()
		const img = screen.getByRole('img', { name: /^France Travail/ })
		expect(img.tagName).toBe('IMG')
		expect(img.getAttribute('src')).toBeTruthy()
	})

	it('shows a lucide icon when there is no logo', () => {
		renderPage()
		const card = screen
			.getByRole('link', { name: /^ATS/ })
			.closest('[data-testid="provider-card"]') as HTMLElement
		expect(within(card).queryByRole('img')).toBeNull()
		expect(card.querySelector('svg.lucide')).toBeTruthy()
	})

	it('clamps the description and keeps the full text in the title', () => {
		renderPage()
		const description = screen.getByText("Offres d'emploi.")
		expect(description.className).toContain('line-clamp-2')
		expect(description.getAttribute('title')).toBe("Offres d'emploi.")
	})

	it('opens a provider when its card is clicked', () => {
		const props = renderPage()
		fireEvent.click(screen.getByRole('link', { name: /^France Travail/ }))
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
