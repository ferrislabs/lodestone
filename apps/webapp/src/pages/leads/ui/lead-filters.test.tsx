import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { filtersFromSearch, type LeadFilters } from '#/pages/leads/lib/search'
import { LeadFiltersForm } from './lead-filters'

afterEach(cleanup)

function renderForm(filters: LeadFilters, onApply = vi.fn()) {
	render(
		<LeadFiltersForm
			profiles={[]}
			profileId={undefined}
			filters={filters}
			onProfileChange={() => {}}
			onApply={onApply}
			onReset={() => {}}
		/>,
	)
	return onApply
}

describe('LeadFiltersForm grouping toggle', () => {
	it('is on by default', () => {
		renderForm(filtersFromSearch({}))
		const toggle = screen.getByLabelText('Regrouper par entreprise')
		expect(toggle).toHaveProperty('checked', true)
	})

	it('reflects a flat listing', () => {
		renderForm(filtersFromSearch({ group: 'none' }))
		expect(screen.getByLabelText('Regrouper par entreprise')).toHaveProperty(
			'checked',
			false,
		)
	})

	it('applies the choice with the other filters', () => {
		const onApply = renderForm(filtersFromSearch({}))
		fireEvent.click(screen.getByLabelText('Regrouper par entreprise'))
		fireEvent.click(screen.getByRole('button', { name: 'Appliquer' }))
		expect(onApply).toHaveBeenCalledTimes(1)
		expect(onApply.mock.calls[0]?.[0]).toMatchObject({ grouped: false })
	})
})
