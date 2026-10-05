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
	it('shows problems above the form and the notice', () => {
		render(
			<ProfileEditor
				form={<p>the form</p>}
				problems={[{ path: 'llm', message: 'is required' }]}
				notice="Revision 2 created"
				dirty
				saving={false}
				saveLabel="Save as new revision"
				onSave={vi.fn()}
			/>,
		)
		const alert = screen.getByRole('alert')
		const form = screen.getByText('the form')
		expect(
			alert.compareDocumentPosition(form) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy()
		expect(screen.getByRole('status').textContent).toBe('Revision 2 created')
	})
})
