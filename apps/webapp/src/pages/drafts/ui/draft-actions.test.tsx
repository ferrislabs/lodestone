import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { MAILTO_MAX_LENGTH } from '#/pages/drafts/lib/mail'
import { DraftActions } from './draft-actions'

afterEach(cleanup)

const TOO_LONG_HINT =
	'Le corps est trop long pour le lien : collez-le avec « Copier le corps ».'

function renderActions(
	overrides: Partial<Parameters<typeof DraftActions>[0]> = {},
) {
	const props = {
		subject: 'Hello Acme',
		body: 'Line one\nLine two',
		contactAddress: 'cto@acme.test' as string | null,
		onCopy: vi.fn(),
		...overrides,
	}
	render(<DraftActions {...props} />)
	return props
}

describe('DraftActions', () => {
	it('copies the subject', () => {
		const props = renderActions()
		fireEvent.click(screen.getByRole('button', { name: "Copier l'objet" }))
		expect(props.onCopy).toHaveBeenCalledWith('Hello Acme')
	})

	it('copies the body', () => {
		const props = renderActions()
		fireEvent.click(screen.getByRole('button', { name: 'Copier le corps' }))
		expect(props.onCopy).toHaveBeenCalledWith('Line one\nLine two')
	})

	it('copies the subject, a blank line, then the body', () => {
		const props = renderActions()
		fireEvent.click(screen.getByRole('button', { name: 'Copier le brouillon' }))
		expect(props.onCopy).toHaveBeenCalledWith(
			'Hello Acme\n\nLine one\nLine two',
		)
	})

	it('links to the mail client when the draft has a contact', () => {
		renderActions()
		expect(
			screen
				.getByRole('link', { name: 'Ouvrir dans la messagerie' })
				.getAttribute('href'),
		).toBe(
			'mailto:cto@acme.test?subject=Hello%20Acme&body=Line%20one%0D%0ALine%20two',
		)
		expect(screen.queryByText(TOO_LONG_HINT)).toBeNull()
	})

	it('has no mail link without a contact and still copies', () => {
		const props = renderActions({ contactAddress: null })
		expect(screen.queryByRole('link')).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: 'Copier le corps' }))
		expect(props.onCopy).toHaveBeenCalledWith('Line one\nLine two')
	})

	it('tells the user to paste a body too long for the link', () => {
		renderActions({ body: 'a'.repeat(MAILTO_MAX_LENGTH) })
		expect(screen.getByText(TOO_LONG_HINT)).toBeTruthy()
		expect(
			screen
				.getByRole('link', { name: 'Ouvrir dans la messagerie' })
				.getAttribute('href'),
		).toBe('mailto:cto@acme.test?subject=Hello%20Acme')
	})

	it('shows no hint without a contact even for a long body', () => {
		renderActions({ contactAddress: null, body: 'a'.repeat(MAILTO_MAX_LENGTH) })
		expect(screen.queryByText(TOO_LONG_HINT)).toBeNull()
	})
})
