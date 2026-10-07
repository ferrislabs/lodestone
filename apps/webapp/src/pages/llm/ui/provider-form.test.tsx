import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { valuesFromProvider } from '#/pages/llm/lib/provider-body'
import type { Provider } from '#/pages/llm/lib/types'
import { ProviderForm } from './provider-form'

afterEach(cleanup)

const provider: Provider = {
	id: 'p1',
	name: 'Claude',
	kind: 'anthropic',
	base_url: 'https://api.anthropic.com',
	model: 'claude-x',
	enabled: true,
	has_api_key: true,
	max_retries: 2,
	timeout_seconds: 30,
	pricing: { input_cents_per_mtok: 300, output_cents_per_mtok: 1500 },
	reasoning_effort: null,
	max_tokens: null,
	temperature: null,
	frequency_penalty: null,
	presence_penalty: null,
	repeat_penalty: null,
	tiers: [],
	created_at: '2026-01-01T00:00:00Z',
	updated_at: '2026-01-01T00:00:00Z',
}

function renderForm(
	overrides: Partial<Parameters<typeof ProviderForm>[0]> = {},
) {
	const props = {
		title: 'Modifier Claude',
		editing: true,
		hasApiKey: true,
		values: valuesFromProvider(provider),
		fieldErrors: {},
		formError: null,
		keyMode: 'keep' as const,
		saving: false,
		dirty: true,
		onChange: vi.fn(),
		onKeyModeChange: vi.fn(),
		onSave: vi.fn(),
		onBack: vi.fn(),
		...overrides,
	}
	render(<ProviderForm {...props} />)
	return props
}

describe('ProviderForm', () => {
	it('shows no key input and no key value when keeping the key', () => {
		renderForm()
		expect(screen.queryByLabelText('Nouvelle clé API')).toBeNull()
		expect(document.querySelector('input[type="password"]')).toBeNull()
		expect(
			screen.getByText("Une clé est définie. Elle n'est jamais affichée."),
		).toBeTruthy()
	})

	it('defaults the tri-state to keep current key', () => {
		renderForm()
		const keep = screen.getByLabelText('Conserver la clé actuelle')
		expect((keep as HTMLInputElement).checked).toBe(true)
		expect(
			(screen.getByLabelText('Remplacer la clé') as HTMLInputElement).checked,
		).toBe(false)
		expect(
			(screen.getByLabelText('Supprimer la clé') as HTMLInputElement).checked,
		).toBe(false)
	})

	it('shows an empty write-only password input when replacing', () => {
		renderForm({ keyMode: 'replace' })
		const input = screen.getByLabelText('Nouvelle clé API') as HTMLInputElement
		expect(input.type).toBe('password')
		expect(input.value).toBe('')
		expect(input.getAttribute('autocomplete')).toBe('new-password')
	})

	it('hides the key input when removing the key', () => {
		renderForm({ keyMode: 'remove' })
		expect(document.querySelector('input[type="password"]')).toBeNull()
	})

	it('reports key mode changes', () => {
		const props = renderForm()
		fireEvent.click(screen.getByLabelText('Remplacer la clé'))
		expect(props.onKeyModeChange).toHaveBeenCalledWith('replace')
	})

	it('shows the api key input on create without a tri-state', () => {
		renderForm({
			editing: false,
			hasApiKey: false,
			values: { ...valuesFromProvider(provider), api_key: '' },
		})
		expect(screen.getByLabelText('Clé API (facultative)')).toBeTruthy()
		expect(screen.queryByLabelText('Conserver la clé actuelle')).toBeNull()
	})

	it('shows server errors on the faulty fields', () => {
		renderForm({
			fieldErrors: {
				base_url: 'must be an http or https URL',
				input_cents_per_mtok: 'must be between 0 and 2147483647',
			},
		})
		expect(screen.getByText('must be an http or https URL')).toBeTruthy()
		expect(screen.getByText('must be between 0 and 2147483647')).toBeTruthy()
		expect(
			screen.getByLabelText('URL de base').getAttribute('aria-invalid'),
		).toBe('true')
	})

	it('offers the reasoning options with the default first', () => {
		renderForm()
		const select = screen.getByLabelText('Raisonnement') as HTMLSelectElement
		expect(Array.from(select.options).map((option) => option.text)).toEqual([
			'Par défaut (non envoyé)',
			'Désactivé',
			'Faible',
			'Moyen',
			'Élevé',
		])
		expect(select.value).toBe('')
		expect(screen.getByText(/« Désactivé » accélère les réponses/)).toBeTruthy()
		expect(
			screen.getByText(/La réflexion du modèle n'est jamais affichée/),
		).toBeTruthy()
		expect(
			screen.getByText(
				/Laissez « Par défaut \(non envoyé\) » pour un modèle sans raisonnement réglable \(ex\. Ministral\)\./,
			),
		).toBeTruthy()
	})

	it('shows the generation parameters with their French labels and help', () => {
		renderForm()
		expect(screen.getByText('Paramètres de génération')).toBeTruthy()
		for (const label of [
			'Tokens maximum',
			'Température',
			'Pénalité de fréquence',
			'Pénalité de présence',
			'Pénalité de répétition',
		]) {
			const input = screen.getByLabelText(label) as HTMLInputElement
			expect(input.value).toBe('')
		}
		expect(screen.getByText(/Laissés vides, ces paramètres/)).toBeTruthy()
		expect(
			screen.getByText(/Ollama l'ignore sur son point d'accès/),
		).toBeTruthy()
		expect(
			screen
				.getByLabelText('Pénalité de répétition')
				.getAttribute('aria-describedby'),
		).toBe('provider-repeat_penalty-help')
	})

	it('shows stored generation values and reports changes', () => {
		const props = renderForm({
			values: {
				...valuesFromProvider({
					...provider,
					max_tokens: 900,
					temperature: 0.2,
					repeat_penalty: 1.3,
				}),
			},
		})
		expect(
			(screen.getByLabelText('Tokens maximum') as HTMLInputElement).value,
		).toBe('900')
		expect(
			(screen.getByLabelText('Température') as HTMLInputElement).value,
		).toBe('0.2')
		fireEvent.change(screen.getByLabelText('Pénalité de répétition'), {
			target: { value: '1,2' },
		})
		expect(props.onChange).toHaveBeenCalledWith('repeat_penalty', '1,2')
	})

	it('shows a generation field error next to its field', () => {
		renderForm({
			fieldErrors: { temperature: 'Doit être compris entre 0 et 2' },
		})
		expect(screen.getByText('Doit être compris entre 0 et 2')).toBeTruthy()
		expect(
			screen.getByLabelText('Température').getAttribute('aria-invalid'),
		).toBe('true')
	})

	it('reports reasoning changes and shows the stored value', () => {
		const props = renderForm({
			values: { ...valuesFromProvider(provider), reasoning_effort: 'low' },
		})
		const select = screen.getByLabelText('Raisonnement') as HTMLSelectElement
		expect(select.value).toBe('low')
		fireEvent.change(select, { target: { value: 'none' } })
		expect(props.onChange).toHaveBeenCalledWith('reasoning_effort', 'none')
	})
})
