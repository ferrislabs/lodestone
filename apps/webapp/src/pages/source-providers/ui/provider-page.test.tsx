import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { detail } from '#/pages/source-providers/lib/fixtures'
import { initialValues } from '#/pages/source-providers/lib/form'
import type { ProviderDetail } from '#/pages/source-providers/lib/types'
import { ProviderPage } from './provider-page'

afterEach(cleanup)

function renderPage(
	current: ProviderDetail = detail(),
	overrides: Partial<Parameters<typeof ProviderPage>[0]> = {},
) {
	const props = {
		detail: current,
		values: initialValues(current),
		secrets: {},
		replacing: [] as string[],
		errors: {},
		formError: null,
		notice: null,
		dirty: true,
		saving: false,
		testing: false,
		connecting: false,
		disconnecting: false,
		deleting: false,
		confirmingDelete: false,
		onChange: vi.fn(),
		onSecretChange: vi.fn(),
		onReplace: vi.fn(),
		onCancelReplace: vi.fn(),
		onSave: vi.fn(),
		onTest: vi.fn(),
		onConnect: vi.fn(),
		onDisconnect: vi.fn(),
		onRequestDelete: vi.fn(),
		onConfirmDelete: vi.fn(),
		onCancelDelete: vi.fn(),
		onBack: vi.fn(),
		...overrides,
	}
	render(<ProviderPage {...props} />)
	return props
}

const AUTHORIZATION_CODE = detail(
	{
		oauth_redirect_uri:
			'https://api.example.com/api/v1/source-providers/oauth/callback',
	},
	{ auth_kind: 'oauth2_authorization_code' },
)

describe('ProviderPage', () => {
	it('generates one input per field specification', () => {
		renderPage()
		expect(
			(screen.getByLabelText('Identifiant client *') as HTMLInputElement).value,
		).toBe('client-1')
		expect(
			(screen.getByLabelText("URL de l'API") as HTMLInputElement).value,
		).toBe('https://api.example.com/v2')
		expect(
			screen.getByText("Identifiant de l'application partenaire."),
		).toBeTruthy()
	})

	it('shows the status, the auth kind and the terms note', () => {
		renderPage()
		expect(screen.getAllByText('Prêt').length).toBeGreaterThan(0)
		expect(screen.getByText('OAuth2 · compte de service')).toBeTruthy()
		expect(
			screen.getByText("Respecter les conditions d'utilisation de l'API."),
		).toBeTruthy()
	})

	it('masks a stored secret and offers to replace it', () => {
		const props = renderPage()
		const masked = screen.getByLabelText(
			'Clé secrète (masqué)',
		) as HTMLInputElement
		expect(masked.value).toBe('••••abcd')
		expect(masked.readOnly).toBe(true)
		expect(document.querySelector('input[type="password"]')).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: 'Remplacer' }))
		expect(props.onReplace).toHaveBeenCalledWith('client_secret')
	})

	it('shows a write-only password input while replacing and can cancel', () => {
		const props = renderPage(detail(), { replacing: ['client_secret'] })
		const input = screen.getByLabelText('Clé secrète *') as HTMLInputElement
		expect(input.type).toBe('password')
		expect(input.value).toBe('')
		fireEvent.change(input, { target: { value: 'next' } })
		expect(props.onSecretChange).toHaveBeenCalledWith('client_secret', 'next')
		fireEvent.click(screen.getByRole('button', { name: 'Annuler' }))
		expect(props.onCancelReplace).toHaveBeenCalledWith('client_secret')
	})

	it('asks for a secret that is not set yet without a replace button', () => {
		renderPage(detail({ secrets: [{ key: 'client_secret', is_set: false }] }))
		expect(
			(screen.getByLabelText('Clé secrète *') as HTMLInputElement).type,
		).toBe('password')
		expect(screen.queryByRole('button', { name: 'Remplacer' })).toBeNull()
	})

	it('saves the form and reports field errors', () => {
		const props = renderPage(detail(), {
			errors: { client_id: 'Champ obligatoire' },
		})
		expect(screen.getByText('Champ obligatoire')).toBeTruthy()
		fireEvent.click(
			screen.getByRole('button', { name: 'Enregistrer et tester' }),
		)
		expect(props.onSave).toHaveBeenCalled()
	})

	it('disables saving when nothing changed', () => {
		renderPage(detail(), { dirty: false })
		expect(
			(
				screen.getByRole('button', {
					name: 'Enregistrer et tester',
				}) as HTMLButtonElement
			).disabled,
		).toBe(true)
	})

	it('tests the connection and shows the last result', () => {
		const props = renderPage()
		expect(screen.getByText('Jeton obtenu et API accessible.')).toBeTruthy()
		fireEvent.click(screen.getByRole('button', { name: 'Tester la connexion' }))
		expect(props.onTest).toHaveBeenCalled()
	})

	it('shows a failing result as an error', () => {
		renderPage(
			detail(
				{ last_test_message: 'Authentification refusée (HTTP 401).' },
				{ status: 'failing' },
			),
		)
		expect(screen.getAllByText('En échec').length).toBeGreaterThan(0)
		expect(
			screen.getByText('Authentification refusée (HTTP 401).').className,
		).toContain('text-destructive')
	})

	it('cannot test an unconfigured provider', () => {
		renderPage(
			detail(
				{ last_test_message: null },
				{ status: 'not_configured', last_test_at: null },
			),
		)
		expect(
			(
				screen.getByRole('button', {
					name: 'Tester la connexion',
				}) as HTMLButtonElement
			).disabled,
		).toBe(true)
		expect(
			screen.getByText("La connexion n'a pas encore été testée."),
		).toBeTruthy()
		expect(
			screen.queryByRole('button', { name: 'Supprimer la configuration' }),
		).toBeNull()
	})

	it('has no OAuth account section for a service account', () => {
		renderPage()
		expect(
			screen.queryByRole('button', { name: 'Connecter un compte' }),
		).toBeNull()
	})

	it('offers to connect an account in the authorization code mode', () => {
		const props = renderPage(AUTHORIZATION_CODE)
		expect(screen.getByText(/oauth\/callback/)).toBeTruthy()
		fireEvent.click(screen.getByRole('button', { name: 'Connecter un compte' }))
		expect(props.onConnect).toHaveBeenCalled()
	})

	it('cannot connect an account before the configuration is saved', () => {
		renderPage(
			detail(
				{ secrets: [{ key: 'client_secret', is_set: false }] },
				{
					auth_kind: 'oauth2_authorization_code',
					status: 'not_configured',
				},
			),
		)
		expect(
			(
				screen.getByRole('button', {
					name: 'Connecter un compte',
				}) as HTMLButtonElement
			).disabled,
		).toBe(true)
	})

	it('shows the connected account and disconnects it', () => {
		const connected = detail(
			{},
			{
				auth_kind: 'oauth2_authorization_code',
				connected_account: {
					display_name: 'Alice Martin',
					account_id: '42',
					scopes: ['read', 'offline'],
					expires_at: '2026-10-08T10:00:00Z',
				},
			},
		)
		const props = renderPage(connected)
		expect(screen.getByText('Alice Martin')).toBeTruthy()
		expect(screen.getByText('42')).toBeTruthy()
		expect(screen.getByText('read, offline')).toBeTruthy()
		expect(
			screen.queryByRole('button', { name: 'Connecter un compte' }),
		).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: 'Déconnecter' }))
		expect(props.onDisconnect).toHaveBeenCalled()
	})

	it('confirms before deleting the configuration', () => {
		const props = renderPage()
		fireEvent.click(
			screen.getByRole('button', { name: 'Supprimer la configuration' }),
		)
		expect(props.onRequestDelete).toHaveBeenCalled()
	})

	it('shows the confirmation step', () => {
		const props = renderPage(detail(), { confirmingDelete: true })
		fireEvent.click(
			screen.getByRole('button', { name: 'Confirmer la suppression' }),
		)
		expect(props.onConfirmDelete).toHaveBeenCalled()
		fireEvent.click(screen.getByRole('button', { name: 'Annuler' }))
		expect(props.onCancelDelete).toHaveBeenCalled()
	})

	it('shows a notice', () => {
		renderPage(detail(), {
			notice: { kind: 'error', text: 'La connexion a échoué.' },
		})
		expect(screen.getByRole('alert').textContent).toBe('La connexion a échoué.')
	})

	it('goes back to the catalog', () => {
		const props = renderPage()
		fireEvent.click(screen.getByRole('button', { name: 'Retour au catalogue' }))
		expect(props.onBack).toHaveBeenCalled()
	})
})
