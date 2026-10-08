import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { ROBOTS_WARNING } from '#/pages/source-providers/lib/crawl'
import { crawlSettings } from '#/pages/source-providers/lib/fixtures'
import { CrawlRulesSheet } from './crawl-rules-panel'

afterEach(cleanup)

function renderSheet(
	overrides: Partial<Parameters<typeof CrawlRulesSheet>[0]> = {},
) {
	const props = {
		open: true,
		onOpenChange: vi.fn(),
		settings: crawlSettings(),
		state: 'ready' as const,
		errorMessage: null,
		saving: false,
		saveError: null,
		retried: null,
		confirming: false,
		onToggle: vi.fn(),
		onConfirm: vi.fn(),
		onCancel: vi.fn(),
		...overrides,
	}
	const view = render(<CrawlRulesSheet {...props} />)
	return { ...props, rerender: view.rerender }
}

function robotsSwitch() {
	return screen.getByRole('switch', {
		name: 'Ignorer robots.txt pour la lecture des sites',
	})
}

describe('CrawlRulesSheet', () => {
	it('shows the switch off, the current state, its origin and the warning', () => {
		renderSheet()
		expect(screen.getByText('Règles de collecte')).toBeTruthy()
		expect(robotsSwitch().getAttribute('aria-checked')).toBe('false')
		expect(
			screen.getByText('robots.txt est respecté pour la lecture des sites.'),
		).toBeTruthy()
		expect(
			screen.getByText(
				'Valeur par défaut du serveur (CRAWL_ROBOTS_POLICY = respect), jamais modifiée ici.',
			),
		).toBeTruthy()
		expect(screen.getByRole('note').textContent).toBe(ROBOTS_WARNING)
	})

	it('shows who switched robots.txt off and when', () => {
		renderSheet({
			settings: crawlSettings({
				robots_policy: 'ignore',
				origin: 'organisation',
				updated_by_name: 'Alice Martin',
				updated_at: '2026-10-09T08:30:00Z',
				robots_ignored: 2,
			}),
			retried: 3,
		})
		expect(robotsSwitch().getAttribute('aria-checked')).toBe('true')
		expect(
			screen.getByText('robots.txt est ignoré pour la lecture des sites.'),
		).toBeTruthy()
		expect(
			screen.getByText('Modifié par Alice Martin le 9 octobre 2026 à 10:30.'),
		).toBeTruthy()
		expect(
			screen.getByText(
				'2 requêtes lues malgré un refus de robots.txt depuis le démarrage du serveur.',
			),
		).toBeTruthy()
		expect(
			screen.getByText('3 dossiers bloqués par robots.txt seront relus.'),
		).toBeTruthy()
	})

	it('reports the switch changes', () => {
		const props = renderSheet()
		fireEvent.click(robotsSwitch())
		expect(props.onToggle).toHaveBeenCalledWith(true)
	})

	it('asks for confirmation before ignoring robots.txt', () => {
		const { rerender, ...props } = renderSheet()
		expect(screen.queryByRole('alertdialog')).toBeNull()
		rerender(<CrawlRulesSheet {...props} confirming />)
		const dialog = screen.getByRole('alertdialog')
		expect(dialog.textContent).toContain('Ignorer robots.txt ?')
		expect(dialog.textContent).toContain(ROBOTS_WARNING)
		fireEvent.click(screen.getByRole('button', { name: 'Ignorer robots.txt' }))
		expect(props.onConfirm).toHaveBeenCalled()
		fireEvent.click(screen.getByRole('button', { name: 'Annuler' }))
		expect(props.onCancel).toHaveBeenCalled()
	})

	it('disables the switch while saving and shows save errors', () => {
		renderSheet({ saving: true, saveError: 'Erreur réseau' })
		expect(robotsSwitch().hasAttribute('disabled')).toBe(true)
		expect(screen.getByRole('alert').textContent).toBe('Erreur réseau')
	})

	it('shows the loading error', () => {
		renderSheet({
			state: 'error',
			settings: null,
			errorMessage: 'Accès refusé',
		})
		expect(screen.getByRole('alert').textContent).toBe('Accès refusé')
		expect(screen.queryByRole('switch')).toBeNull()
	})
})
