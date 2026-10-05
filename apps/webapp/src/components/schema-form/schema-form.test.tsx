import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { SchemaForm } from '#/components/schema-form/schema-form'
import {
	defaultValue,
	type JsonValue,
	type Problem,
	setIn,
} from '#/lib/schema-form'
import { loadProfileSchema } from '#/lib/schema-form/test-schema'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'

afterEach(cleanup)

const root = loadProfileSchema()

interface HarnessProps {
	initial?: JsonValue
	problems?: readonly Problem[]
	onValue?: (value: JsonValue) => void
}

function Harness({ initial, problems = [], onValue }: HarnessProps) {
	const [value, setValue] = useState<JsonValue>(
		initial ?? defaultValue(root, root, PROFILE_HINTS),
	)
	return (
		<SchemaForm
			context={{
				root,
				hints: PROFILE_HINTS,
				value,
				problems,
				onChange: (next) => {
					setValue(next)
					onValue?.(next)
				},
			}}
		/>
	)
}

describe('SchemaForm', () => {
	it('marks required fields and hides the schema version', () => {
		render(<Harness />)
		const offer = screen.getByLabelText(/Offer/)
		expect(offer.tagName).toBe('TEXTAREA')
		expect(screen.getByText('Offer').parentElement?.textContent).toContain('*')
		expect(screen.getAllByTitle('Required').length).toBeGreaterThan(5)
		expect(screen.queryByLabelText(/Schema version/)).toBeNull()
		expect(screen.getByLabelText(/Qualification threshold/)).toHaveProperty(
			'value',
			'50',
		)
	})

	it('adds and removes a row of a list', () => {
		render(<Harness />)
		fireEvent.click(screen.getByLabelText('Add Preferred local parts'))
		const input = screen.getByLabelText('Preferred local parts 1')
		fireEvent.change(input, { target: { value: 'contact' } })
		expect(screen.getByLabelText('Preferred local parts 1')).toHaveProperty(
			'value',
			'contact',
		)
		fireEvent.click(screen.getByLabelText('Add Preferred local parts'))
		expect(screen.getByLabelText('Preferred local parts 2')).toBeTruthy()
		fireEvent.click(screen.getByLabelText('Remove Preferred local parts 1'))
		expect(screen.queryByLabelText('Preferred local parts 2')).toBeNull()
	})

	it('adds a card for lists of objects', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByLabelText('Add Factors'))
		expect(screen.getByText('Scoring factor 1')).toBeTruthy()
		const last = seen.mock.calls.at(-1)?.[0]
		expect(last).toMatchObject({
			scoring: { factors: [{ signal: '', weight: 5 }] },
		})
	})

	it('toggles optional sections and omits the key when off', () => {
		const initial = setIn(
			defaultValue(root, root, PROFILE_HINTS),
			['agents'],
			[
				{
					id: 'a',
					name: 'A',
					enabled: true,
					source: { kind: 'ats', keywords: ['x'], exclusions: [] },
				},
			],
		)
		const seen = vi.fn()
		render(<Harness initial={initial} onValue={seen} />)
		const before = screen.getAllByLabelText(/Relevance criteria/).length
		fireEvent.click(screen.getByLabelText('Enable LLM'))
		expect(screen.getAllByLabelText(/Relevance criteria/)).toHaveLength(
			before + 1,
		)
		expect(seen.mock.calls.at(-1)?.[0]).toMatchObject({
			agents: [{ llm: {} }],
		})
		fireEvent.click(screen.getByLabelText('Enable LLM'))
		const last = seen.mock.calls.at(-1)?.[0]
		expect(last).not.toHaveProperty('agents.0.llm')
		expect(screen.getAllByLabelText(/Relevance criteria/)).toHaveLength(before)
	})

	it('switches union variants and resets their fields', () => {
		const initial = setIn(
			defaultValue(root, root, PROFILE_HINTS),
			['agents'],
			[
				{
					id: 'a',
					name: 'A',
					enabled: true,
					source: {
						kind: 'http_probe',
						fingerprints: [
							{
								id: 'a',
								path: '/',
								product: 'p',
								matchers: [{ kind: 'header', name: 'x', pattern: 'y' }],
							},
						],
					},
				},
			],
		)
		const seen = vi.fn()
		render(<Harness initial={initial} onValue={seen} />)
		expect(screen.getAllByLabelText('Name *')).toHaveLength(2)
		fireEvent.change(screen.getByLabelText('Matcher type'), {
			target: { value: '2' },
		})
		expect(screen.getAllByLabelText('Name *')).toHaveLength(1)
		expect(screen.getByLabelText(/Pattern/)).toHaveProperty('value', '')
		expect(seen.mock.calls.at(-1)?.[0]).toMatchObject({
			agents: [
				{
					source: {
						fingerprints: [{ matchers: [{ kind: 'body', pattern: '' }] }],
					},
				},
			],
		})
	})

	it('shows a server error under the faulty field', () => {
		const initial = setIn(
			defaultValue(root, root, PROFILE_HINTS),
			['scoring', 'factors'],
			[{ signal: 's', weight: 1 }],
		)
		render(
			<Harness
				initial={initial}
				problems={[
					{ path: 'scoring.factors[0].weight', message: 'must be positive' },
				]}
			/>,
		)
		const weight = screen.getByLabelText(/Weight/)
		const message = screen.getByText('must be positive')
		expect(weight.getAttribute('aria-invalid')).toBe('true')
		expect(weight.getAttribute('aria-describedby')).toBe(
			message.parentElement?.id,
		)
		const frame = weight.parentElement as HTMLElement
		expect(within(frame).getByText('must be positive')).toBe(message)
	})
})
