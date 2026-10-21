import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { JsonObject } from '#/lib/schema-form'
import { DefinitionTabContent } from '#/pages/profiles/feature/definition-form'
import { IAM } from '#/pages/profiles/lib/scoring-fixture'
import type { Problem } from '#/pages/profiles/lib/types'

afterEach(cleanup)

function withFactors(factors: JsonObject[]): JsonObject {
	return { ...IAM, scoring: { factors } }
}

function Harness({
	initial,
	problems = [],
	tab = 'scoring',
}: {
	initial: JsonObject
	problems?: readonly Problem[]
	tab?: 'scoring' | 'settings'
}) {
	const [value, setValue] = useState(initial)
	return (
		<>
			<DefinitionTabContent
				tab={tab}
				value={value}
				problems={problems}
				onChange={setValue}
			/>
			<pre data-testid="json">{JSON.stringify(value.scoring)}</pre>
			<pre data-testid="llm">{JSON.stringify(value.llm)}</pre>
		</>
	)
}

const FACTORS: JsonObject[] = [
	{ signal: 'product_detected', weight: 10, value: 'keycloak' },
	{ signal: 'job_offer', weight: 8 },
	{ signal: 'distinct_signals', weight: 2 },
]

describe('scoring grid', () => {
	it('renders one card per factor with its title', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		expect(screen.getAllByRole('heading', { level: 3 })).toHaveLength(3)
		expect(screen.getByText('Produit détecté · keycloak')).toBeTruthy()
		expect(
			screen.getAllByText('3 facteurs · score max 22 points'),
		).toHaveLength(1)
		expect(screen.queryByRole('textbox', { name: /json/i })).toBeNull()
	})

	it('hides the value for kinds that take none', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		expect(screen.getAllByLabelText('Valeur')).toHaveLength(1)
		expect(
			screen.getByText('2 points par type de signal distinct'),
		).toBeTruthy()
	})

	it('suggests values from the agents', () => {
		const { container } = render(<Harness initial={withFactors(FACTORS)} />)
		const options = [
			...container.querySelectorAll('datalist#factor-values-0 option'),
		].map((option) => option.getAttribute('value'))
		expect(options).toContain('keycloak')
	})

	it('warns when no agent emits the signal', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		expect(
			screen.getAllByText("Aucun agent n'émet encore ce signal"),
		).toHaveLength(1)
	})

	it('adds a factor and focuses its weight', async () => {
		render(<Harness initial={withFactors([])} />)
		expect(
			screen.getByText(
				'Aucun facteur de scoring. Tous les leads auront un score de 0.',
			),
		).toBeTruthy()
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un facteur' }))
		expect(screen.getByTestId('json').textContent).toBe(
			'{"factors":[{"signal":"product_detected","weight":5}]}',
		)
		await waitFor(() => {
			const active = document.activeElement
			expect(active?.getAttribute('role')).toBe('radio')
			expect(active?.textContent).toBe('5')
			expect(active?.getAttribute('aria-checked')).toBe('true')
		})
	})

	it('renders a 1 to 10 radiogroup per factor with the weight selected', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		const groups = screen.getAllByRole('radiogroup', { name: 'Poids' })
		expect(groups).toHaveLength(3)
		const radios = within(groups[0] as HTMLElement).getAllByRole('radio')
		expect(radios.map((radio) => radio.textContent)).toEqual(
			Array.from({ length: 10 }, (_, index) => String(index + 1)),
		)
		const checked = radios.filter(
			(radio) => radio.getAttribute('aria-checked') === 'true',
		)
		expect(checked.map((radio) => radio.textContent)).toEqual(['10'])
	})

	it('changes the weight when a button is clicked', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		const group = screen.getAllByRole('radiogroup')[1] as HTMLElement
		fireEvent.click(within(group).getByRole('radio', { name: '3' }))
		expect(screen.getByTestId('json').textContent).toContain(
			'{"signal":"job_offer","weight":3}',
		)
		expect(
			within(group)
				.getByRole('radio', { name: '3' })
				.getAttribute('aria-checked'),
		).toBe('true')
		expect(screen.getByText("Ajoute jusqu'à 3 points")).toBeTruthy()
	})

	it('moves the selection with the arrow keys', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		const group = screen.getAllByRole('radiogroup')[1] as HTMLElement
		const selected = within(group).getByRole('radio', { name: '8' })
		fireEvent.keyDown(selected, { key: 'ArrowRight' })
		expect(screen.getByTestId('json').textContent).toContain(
			'{"signal":"job_offer","weight":9}',
		)
		fireEvent.keyDown(within(group).getByRole('radio', { name: '9' }), {
			key: 'ArrowLeft',
		})
		fireEvent.keyDown(within(group).getByRole('radio', { name: '8' }), {
			key: 'ArrowLeft',
		})
		expect(screen.getByTestId('json').textContent).toContain(
			'{"signal":"job_offer","weight":7}',
		)
		expect(document.activeElement?.textContent).toBe('7')
	})

	it('keeps the selection inside 1 to 10', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		const group = screen.getAllByRole('radiogroup')[0] as HTMLElement
		fireEvent.keyDown(within(group).getByRole('radio', { name: '10' }), {
			key: 'ArrowRight',
		})
		expect(screen.getByTestId('json').textContent).toContain(
			'"weight":10,"value":"keycloak"',
		)
	})

	it('shows no selection and an error for a weight outside 1 to 10', () => {
		render(
			<Harness
				initial={withFactors([{ signal: 'job_offer', weight: 30 }, ...FACTORS])}
			/>,
		)
		const group = screen.getAllByRole('radiogroup')[0] as HTMLElement
		expect(
			within(group)
				.getAllByRole('radio')
				.some((radio) => radio.getAttribute('aria-checked') === 'true'),
		).toBe(false)
		expect(screen.getByText('Doit être un entier de 1 à 10')).toBeTruthy()
		expect(group.getAttribute('aria-invalid')).toBe('true')
	})

	it('explains the points a weight can bring', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		expect(screen.getByText("Ajoute jusqu'à 10 points")).toBeTruthy()
		expect(screen.getByText("Ajoute jusqu'à 8 points")).toBeTruthy()
	})

	it('removes a factor', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		fireEvent.click(
			screen.getByRole('button', {
				name: 'Supprimer le facteur Produit détecté · keycloak',
			}),
		)
		expect(screen.getAllByRole('heading', { level: 3 })).toHaveLength(2)
	})

	it('unsets the value when cleared', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		fireEvent.change(screen.getByLabelText('Valeur'), { target: { value: '' } })
		expect(screen.getByTestId('json').textContent).not.toContain('"value"')
	})

	it('shows problems under the weight', () => {
		render(
			<Harness
				initial={withFactors(FACTORS)}
				problems={[
					{ path: 'scoring.factors[0].weight', message: 'must be positive' },
				]}
			/>,
		)
		expect(screen.getByText('must be positive')).toBeTruthy()
	})

	it('shows the qualification threshold in percent with its points', () => {
		render(
			<Harness
				tab="settings"
				initial={{
					...withFactors(FACTORS),
					llm: { qualification_threshold: 50, relevance_criteria: 'x' },
				}}
			/>,
		)
		const input = screen.getByLabelText(
			'Seuil de qualification',
		) as HTMLInputElement
		expect(input.value).toBe('50')
		expect(input.min).toBe('0')
		expect(input.max).toBe('100')
		expect(screen.getByText('%')).toBeTruthy()
		expect(
			screen.getByText('= 11 points avec les facteurs actuels (max 22)'),
		).toBeTruthy()
	})

	it('updates the points while the percentage is edited', () => {
		render(
			<Harness
				tab="settings"
				initial={{
					...withFactors(FACTORS),
					llm: { qualification_threshold: 50, relevance_criteria: 'x' },
				}}
			/>,
		)
		fireEvent.change(screen.getByLabelText('Seuil de qualification'), {
			target: { value: '35' },
		})
		expect(screen.getByTestId('llm').textContent).toContain(
			'"qualification_threshold":35',
		)
		expect(
			screen.getByText('= 7,7 points avec les facteurs actuels (max 22)'),
		).toBeTruthy()
	})

	it('flags a threshold above 100', () => {
		render(
			<Harness
				tab="settings"
				initial={{
					...withFactors(FACTORS),
					llm: { qualification_threshold: 250, relevance_criteria: 'x' },
				}}
			/>,
		)
		expect(screen.getByText('Doit être un pourcentage de 0 à 100')).toBeTruthy()
		expect(screen.queryByText(/avec les facteurs actuels/)).toBeNull()
	})

	it('explains the scoring with the live numbers of the profile', () => {
		const navigate = vi.fn()
		render(
			<DefinitionTabContent
				tab="scoring"
				value={{
					...withFactors(FACTORS),
					llm: { qualification_threshold: 50, relevance_criteria: 'x' },
				}}
				problems={[]}
				onChange={() => {}}
				onNavigate={navigate}
			/>,
		)
		const panel = screen.getByLabelText('Fonctionnement du scoring')
		expect(
			within(panel).getByText('Seuil actuel : 50 % = 11 points'),
		).toBeTruthy()
		expect(within(panel).getByTestId('scoring-example').textContent).toContain(
			'10 × 0,9 + 8 × 0,7 = 14,6 points',
		)
		fireEvent.click(
			within(panel).getByRole('button', { name: 'Modifier le seuil' }),
		)
		expect(navigate).toHaveBeenCalledWith('settings')
	})

	it('hides the example with fewer than two factors', () => {
		render(<Harness initial={withFactors([FACTORS[0] as JsonObject])} />)
		expect(screen.queryByTestId('scoring-example')).toBeNull()
	})

	it.each([
		'scoring',
		'sourcing',
	] as const)('puts the help panel in the right column of the %s tab', (tab) => {
		render(
			<DefinitionTabContent
				tab={tab}
				value={withFactors(FACTORS)}
				problems={[]}
				onChange={() => {}}
			/>,
		)
		const panels = screen.getAllByRole('complementary')
		const layout = screen.getByTestId('help-layout')
		expect(layout.className).toContain('lg:grid-cols-4')
		expect(panels).toHaveLength(tab === 'scoring' ? 2 : 1)
		for (const panel of panels) expect(layout.contains(panel)).toBe(true)
		expect(panels[0]?.className).toContain('lg:col-span-1')
		expect(panels[0]?.className).toContain('lg:sticky')
		if (tab === 'scoring') {
			expect(panels[0]?.getAttribute('aria-label')).toBe('Scoring')
			expect(panels[1]?.getAttribute('aria-label')).toBe(
				'Fonctionnement du scoring',
			)
			expect(panels[0]?.parentElement).toBe(panels[1]?.parentElement)
			expect(panels[0]?.parentElement?.className).toContain('lg:col-span-1')
		}
	})
})
