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
	tiers: [],
	created_at: '2026-01-01T00:00:00Z',
	updated_at: '2026-01-01T00:00:00Z',
}

function renderForm(
	overrides: Partial<Parameters<typeof ProviderForm>[0]> = {},
) {
	const props = {
		title: 'Edit Claude',
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
		expect(screen.queryByLabelText('New API key')).toBeNull()
		expect(document.querySelector('input[type="password"]')).toBeNull()
		expect(screen.getByText('A key is set. It is never shown.')).toBeTruthy()
	})

	it('defaults the tri-state to keep current key', () => {
		renderForm()
		const keep = screen.getByLabelText('Keep current key')
		expect((keep as HTMLInputElement).checked).toBe(true)
		expect(
			(screen.getByLabelText('Replace key') as HTMLInputElement).checked,
		).toBe(false)
		expect(
			(screen.getByLabelText('Remove key') as HTMLInputElement).checked,
		).toBe(false)
	})

	it('shows an empty write-only password input when replacing', () => {
		renderForm({ keyMode: 'replace' })
		const input = screen.getByLabelText('New API key') as HTMLInputElement
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
		fireEvent.click(screen.getByLabelText('Replace key'))
		expect(props.onKeyModeChange).toHaveBeenCalledWith('replace')
	})

	it('shows the api key input on create without a tri-state', () => {
		renderForm({
			editing: false,
			hasApiKey: false,
			values: { ...valuesFromProvider(provider), api_key: '' },
		})
		expect(screen.getByLabelText('API key (optional)')).toBeTruthy()
		expect(screen.queryByLabelText('Keep current key')).toBeNull()
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
		expect(screen.getByLabelText('Base URL').getAttribute('aria-invalid')).toBe(
			'true',
		)
	})
})
