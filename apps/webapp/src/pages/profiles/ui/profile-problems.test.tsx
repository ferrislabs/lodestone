import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { ProfileEditor } from './profile-editor'
import { ProfileProblems } from './profile-problems'

afterEach(cleanup)

describe('ProfileProblems', () => {
	it('lists every problem with its path', () => {
		render(
			<ProfileProblems
				problems={[
					{ path: 'scoring.factors[2].weight', message: 'must be positive' },
					{ path: '$', message: 'malformed JSON: unexpected end' },
				]}
			/>,
		)
		expect(screen.getByText('scoring.factors[2].weight')).toBeTruthy()
		expect(screen.getByText(/must be positive/)).toBeTruthy()
		expect(screen.getByText('$')).toBeTruthy()
		expect(screen.getAllByRole('listitem').length).toBe(2)
	})

	it('renders nothing without problems', () => {
		render(<ProfileProblems problems={[]} />)
		expect(screen.queryByRole('alert')).toBeNull()
	})
})

describe('ProfileEditor', () => {
	it('shows problems above the editor and keeps the text', () => {
		render(
			<ProfileEditor
				text='{"a":1}'
				problems={[{ path: 'llm', message: 'is required' }]}
				notice="Revision 2 created"
				dirty
				saving={false}
				saveLabel="Save as new revision"
				onTextChange={vi.fn()}
				onSave={vi.fn()}
			/>,
		)
		const alert = screen.getByRole('alert')
		const textarea = screen.getByLabelText('Definition (JSON)')
		expect(
			alert.compareDocumentPosition(textarea) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy()
		expect((textarea as HTMLTextAreaElement).value).toBe('{"a":1}')
		expect(screen.getByRole('status').textContent).toBe('Revision 2 created')
	})
})
