import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it } from 'vitest'

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
}: {
	initial: JsonObject
	problems?: readonly Problem[]
}) {
	const [value, setValue] = useState(initial)
	return (
		<>
			<DefinitionTabContent
				tab="scoring"
				value={value}
				problems={problems}
				onChange={setValue}
			/>
			<pre data-testid="json">{JSON.stringify(value.scoring)}</pre>
		</>
	)
}

const FACTORS: JsonObject[] = [
	{ signal: 'product_detected', weight: 30, value: 'keycloak' },
	{ signal: 'job_offer', weight: 10 },
	{ signal: 'distinct_signals', weight: 5 },
]

describe('scoring grid', () => {
	it('renders one card per factor with its title', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		expect(screen.getAllByRole('heading', { level: 3 })).toHaveLength(3)
		expect(screen.getByText('Product detected · keycloak')).toBeTruthy()
		expect(screen.getByText('3 factors · max weight 30')).toBeTruthy()
		expect(screen.queryByRole('textbox', { name: /json/i })).toBeNull()
	})

	it('hides the value for kinds that take none', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		expect(screen.getAllByLabelText('Value')).toHaveLength(1)
		expect(screen.getByText('points per distinct signal kind')).toBeTruthy()
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
		expect(screen.getAllByText('No agent emits this signal yet')).toHaveLength(
			1,
		)
	})

	it('adds a factor and focuses its weight', async () => {
		render(<Harness initial={withFactors([])} />)
		expect(
			screen.getByText('No scoring factor. Leads will all score 0.'),
		).toBeTruthy()
		fireEvent.click(screen.getByRole('button', { name: 'Add factor' }))
		expect(screen.getByTestId('json').textContent).toBe(
			'{"factors":[{"signal":"product_detected","weight":10}]}',
		)
		await waitFor(() =>
			expect(document.activeElement).toBe(screen.getByLabelText('Weight')),
		)
	})

	it('lets the weight be cleared before typing a new one', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		const weight = screen.getAllByLabelText('Weight')[0] as HTMLInputElement
		fireEvent.change(weight, { target: { value: '' } })
		expect(weight.value).toBe('')
		fireEvent.change(weight, { target: { value: '45' } })
		expect(weight.value).toBe('45')
	})

	it('removes a factor', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		fireEvent.click(
			screen.getByRole('button', {
				name: 'Remove factor Product detected · keycloak',
			}),
		)
		expect(screen.getAllByRole('heading', { level: 3 })).toHaveLength(2)
	})

	it('unsets the value when cleared', () => {
		render(<Harness initial={withFactors(FACTORS)} />)
		fireEvent.change(screen.getByLabelText('Value'), { target: { value: '' } })
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
})
