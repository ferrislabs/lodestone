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

describe('LeadFiltersForm sort', () => {
	it('sorts by the score of the scoring agents and keeps the signal score apart', () => {
		renderForm(filtersFromSearch({}))
		const select = screen.getByLabelText('Tri') as HTMLSelectElement
		const options = Array.from(select.options).map((option) => [
			option.value,
			option.textContent,
		])
		expect(options).toContainEqual(['llm_score_desc', 'Score décroissant'])
		expect(options).toContainEqual(['llm_score_asc', 'Score croissant'])
		expect(options).toContainEqual([
			'score_desc',
			'Force des signaux décroissante',
		])
		expect(options).toContainEqual([
			'score_asc',
			'Force des signaux croissante',
		])
	})

	it('filters by a minimum signal strength from 0 to 100', () => {
		const onApply = renderForm(filtersFromSearch({}))
		const input = screen.getByLabelText(
			'Force des signaux minimale (0 à 100)',
		) as HTMLInputElement
		expect(input.min).toBe('0')
		expect(input.max).toBe('100')
		fireEvent.change(input, { target: { value: '40' } })
		fireEvent.click(screen.getByRole('button', { name: 'Appliquer' }))
		expect(onApply.mock.calls[0]?.[0]).toMatchObject({ minScore: '40' })
	})

	it('applies the chosen score sort', () => {
		const onApply = renderForm(filtersFromSearch({}))
		fireEvent.change(screen.getByLabelText('Tri'), {
			target: { value: 'llm_score_desc' },
		})
		fireEvent.click(screen.getByRole('button', { name: 'Appliquer' }))
		expect(onApply.mock.calls[0]?.[0]).toMatchObject({ sort: 'llm_score_desc' })
	})
})
