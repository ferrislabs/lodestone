import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it } from 'vitest'

import { TagInput } from '#/components/ui/tag-input'

afterEach(cleanup)

function Harness({
	initial = [],
	invalid = [],
}: {
	initial?: string[]
	invalid?: number[]
}) {
	const [values, setValues] = useState(initial)
	return (
		<>
			<TagInput
				id="t"
				label="Mots-clés"
				values={values}
				invalid={invalid}
				onChange={setValues}
			/>
			<output data-testid="values">{JSON.stringify(values)}</output>
		</>
	)
}

function state() {
	return screen.getByTestId('values').textContent
}

function input() {
	return screen.getByLabelText('Mots-clés') as HTMLInputElement
}

describe('TagInput', () => {
	it('adds a trimmed value on Enter and clears the input', () => {
		render(<Harness />)
		input().value = '  sso '
		fireEvent.keyDown(input(), { key: 'Enter' })
		expect(state()).toBe('["sso"]')
		expect(input().value).toBe('')
	})

	it('adds a value on comma and ignores empty input', () => {
		render(<Harness />)
		fireEvent.keyDown(input(), { key: 'Enter' })
		input().value = 'auth'
		fireEvent.keyDown(input(), { key: ',' })
		expect(state()).toBe('["auth"]')
	})

	it('splits pasted text on commas, semicolons and newlines', () => {
		render(<Harness initial={['a']} />)
		fireEvent.paste(input(), {
			clipboardData: { getData: () => 'b, c;d\ne,a' },
		})
		expect(state()).toBe('["a","b","c","d","e"]')
	})

	it('ignores case-insensitive duplicates and flags them', () => {
		render(<Harness initial={['Auth']} />)
		input().value = 'auth'
		fireEvent.keyDown(input(), { key: 'Enter' })
		expect(state()).toBe('["Auth"]')
		expect(input().closest<HTMLElement>('[data-tag-input]')?.dataset.dup).toBe(
			'true',
		)
		fireEvent.change(input(), { target: { value: 'x' } })
		expect(
			input().closest<HTMLElement>('[data-tag-input]')?.dataset.dup,
		).toBeUndefined()
	})

	it('removes the last chip on Backspace only when the input is empty', () => {
		render(<Harness initial={['a', 'b']} />)
		input().value = 'x'
		fireEvent.keyDown(input(), { key: 'Backspace' })
		expect(state()).toBe('["a","b"]')
		input().value = ''
		fireEvent.keyDown(input(), { key: 'Backspace' })
		expect(state()).toBe('["a"]')
	})

	it('removes a chip with its cross button', () => {
		render(<Harness initial={['a', 'b']} />)
		fireEvent.click(screen.getByRole('button', { name: 'Retirer a' }))
		expect(state()).toBe('["b"]')
	})

	it('marks invalid chips', () => {
		render(<Harness initial={['a', 'b']} invalid={[1]} />)
		const chips = document.querySelectorAll<HTMLElement>('[data-tag]')
		expect(chips[0]?.getAttribute('aria-invalid')).toBe('false')
		expect(chips[1]?.getAttribute('aria-invalid')).toBe('true')
		expect(within(chips[1] as HTMLElement).getByText('b')).toBeTruthy()
	})
})
