import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it } from 'vitest'

import type { JsonObject } from '#/lib/schema-form'
import { DefinitionTabContent } from '#/pages/profiles/feature/definition-form'

afterEach(cleanup)

function Harness({
	initial,
	tab = 'settings',
}: {
	initial: JsonObject
	tab?: 'scoring' | 'sourcing' | 'settings'
}) {
	const [value, setValue] = useState(initial)
	return (
		<>
			<DefinitionTabContent
				tab={tab}
				value={value}
				problems={[]}
				onChange={setValue}
			/>
			<pre data-testid="llm">{JSON.stringify(value.llm)}</pre>
		</>
	)
}

function withThreshold(threshold: number): JsonObject {
	return {
		agents: [],
		llm: { qualification_threshold: threshold, relevance_criteria: 'x' },
	}
}

describe('qualification threshold', () => {
	it('shows the qualification threshold in percent', () => {
		render(<Harness initial={withThreshold(50)} />)
		const input = screen.getByLabelText(
			'Seuil de qualification',
		) as HTMLInputElement
		expect(input.value).toBe('50')
		expect(input.min).toBe('0')
		expect(input.max).toBe('100')
		expect(screen.getByText('%')).toBeTruthy()
		expect(screen.queryByText(/points/)).toBeNull()
	})

	it('writes the edited percentage into the definition', () => {
		render(<Harness initial={withThreshold(50)} />)
		fireEvent.change(screen.getByLabelText('Seuil de qualification'), {
			target: { value: '35' },
		})
		expect(screen.getByTestId('llm').textContent).toContain(
			'"qualification_threshold":35',
		)
	})

	it('flags a threshold above 100', () => {
		render(<Harness initial={withThreshold(250)} />)
		expect(screen.getByText('Doit être un pourcentage de 0 à 100')).toBeTruthy()
	})
})

describe('scoring tab without signal weights', () => {
	it('shows the scoring agents and only the scoring help', () => {
		render(<Harness tab="scoring" initial={withThreshold(50)} />)
		expect(
			screen.getByRole('region', { name: 'Agents de scoring' }),
		).toBeTruthy()
		expect(screen.queryByText('Poids des signaux (sourcing)')).toBeNull()
		expect(screen.queryByLabelText('Facteurs de scoring')).toBeNull()
		expect(screen.queryByLabelText('Fonctionnement du scoring')).toBeNull()
		expect(screen.queryByText('Modifier le seuil')).toBeNull()
	})

	it('explains the signal strength in the scoring help', () => {
		render(<Harness tab="scoring" initial={withThreshold(50)} />)
		const help = screen.getByLabelText('Scoring')
		expect(help.textContent).toContain('La force des signaux (0 à 100)')
		expect(help.textContent).toContain('trois signaux indépendants')
		expect(help.textContent).toContain("Force du signal selon l'origine")
	})

	it.each([
		'scoring',
		'sourcing',
	] as const)('puts the help panel in the right column of the %s tab', (tab) => {
		render(<Harness tab={tab} initial={withThreshold(50)} />)
		const panels = screen.getAllByRole('complementary')
		const layout = screen.getByTestId('help-layout')
		expect(layout.className).toContain('lg:grid-cols-4')
		expect(panels).toHaveLength(1)
		expect(layout.contains(panels[0] as HTMLElement)).toBe(true)
		expect(panels[0]?.className).toContain('lg:col-span-1')
		expect(panels[0]?.className).toContain('lg:sticky')
	})
})
