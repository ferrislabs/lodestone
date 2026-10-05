import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Provider } from '#/pages/llm/lib/types'
import { TierPanel } from './tier-panel'

afterEach(cleanup)

function provider(id: string, name: string, enabled: boolean): Provider {
	return {
		id,
		name,
		kind: 'anthropic',
		base_url: 'https://x.test',
		model: 'm',
		enabled,
		has_api_key: false,
		max_retries: 2,
		timeout_seconds: 30,
		pricing: { input_cents_per_mtok: 0, output_cents_per_mtok: 0 },
		tiers: [],
		created_at: '2026-01-01T00:00:00Z',
		updated_at: '2026-01-01T00:00:00Z',
	}
}

function renderPanel(overrides: Partial<Parameters<typeof TierPanel>[0]> = {}) {
	const props = {
		providers: [provider('a', 'Claude', true), provider('b', 'Mistral', false)],
		selection: { triage: 'a', qualification: '' },
		changed: false,
		applying: false,
		error: null,
		onSelect: vi.fn(),
		onApply: vi.fn(),
		...overrides,
	}
	render(<TierPanel {...props} />)
	return props
}

describe('TierPanel', () => {
	it('shows the current selection per tier', () => {
		renderPanel()
		expect((screen.getByLabelText('Tri') as HTMLSelectElement).value).toBe('a')
		expect(
			(screen.getByLabelText('Qualification') as HTMLSelectElement).value,
		).toBe('')
	})

	it('offers none and marks disabled providers', () => {
		renderPanel()
		expect(screen.getAllByText('Aucun').length).toBe(2)
		expect(screen.getAllByText('Mistral (désactivé)').length).toBe(2)
	})

	it('reports a selection change', () => {
		const props = renderPanel()
		fireEvent.change(screen.getByLabelText('Qualification'), {
			target: { value: 'a' },
		})
		expect(props.onSelect).toHaveBeenCalledWith('qualification', 'a')
	})

	it('applies only when something changed', () => {
		const props = renderPanel({ changed: true })
		fireEvent.click(screen.getByText('Appliquer les niveaux'))
		expect(props.onApply).toHaveBeenCalled()
		cleanup()
		renderPanel({ changed: false })
		expect(
			(screen.getByText('Appliquer les niveaux') as HTMLButtonElement).disabled,
		).toBe(true)
	})
})
