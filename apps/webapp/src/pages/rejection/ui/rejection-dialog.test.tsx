import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { RejectionDialog } from './rejection-dialog'

afterEach(cleanup)

function renderDialog(
	overrides: Partial<Parameters<typeof RejectionDialog>[0]> = {},
) {
	const props = {
		open: true,
		title: 'Rejeter ce lead',
		description: 'Description',
		reason: null,
		note: '',
		errors: { reason: null, note: null },
		formError: null,
		pending: false,
		onReasonChange: vi.fn(),
		onNoteChange: vi.fn(),
		onConfirm: vi.fn(),
		onCancel: vi.fn(),
		...overrides,
	}
	render(<RejectionDialog {...props} />)
	return props
}

describe('RejectionDialog', () => {
	it('lists the seven reasons in French', () => {
		renderDialog()
		expect(screen.getByRole('dialog')).toBeTruthy()
		expect(screen.getByText('Rejeter ce lead')).toBeTruthy()
		for (const label of [
			'Hors cible',
			'Déjà client',
			'Concurrent',
			'Éditeur ou intégrateur du domaine',
			'Hébergeur ou agence (le signal concerne un client)',
			'Mauvaise donnée',
			'Autre',
		]) {
			expect(screen.getByLabelText(label)).toBeTruthy()
		}
		expect(screen.getAllByRole('radio')).toHaveLength(7)
	})

	it('reports the chosen reason and the typed note', () => {
		const props = renderDialog()
		fireEvent.click(screen.getByLabelText('Concurrent'))
		expect(props.onReasonChange).toHaveBeenCalledWith('concurrent')
		fireEvent.change(screen.getByLabelText('Précision (facultatif)'), {
			target: { value: 'notre rival' },
		})
		expect(props.onNoteChange).toHaveBeenCalledWith('notre rival')
	})

	it('makes the note mandatory for autre', () => {
		renderDialog({ reason: 'autre' })
		expect(screen.getByLabelText('Précision')).toBeTruthy()
		expect(screen.queryByLabelText('Précision (facultatif)')).toBeNull()
	})

	it('shows the field errors and the form error', () => {
		renderDialog({
			reason: 'autre',
			errors: {
				reason: 'Choisissez une raison de rejet',
				note: 'Précisez la raison quand elle est « Autre »',
			},
			formError: 'Le statut du lead a changé',
		})
		expect(screen.getByText('Choisissez une raison de rejet')).toBeTruthy()
		const note = screen.getByLabelText('Précision')
		expect(note.getAttribute('aria-invalid')).toBe('true')
		expect(
			document.getElementById(note.getAttribute('aria-describedby') ?? '')
				?.textContent,
		).toBe('Précisez la raison quand elle est « Autre »')
		expect(screen.getByText('Le statut du lead a changé')).toBeTruthy()
	})

	it('confirms and cancels', () => {
		const props = renderDialog({ reason: 'hors_cible' })
		fireEvent.click(screen.getByText('Confirmer le rejet'))
		expect(props.onConfirm).toHaveBeenCalled()
		fireEvent.click(screen.getByText('Annuler'))
		expect(props.onCancel).toHaveBeenCalled()
	})

	it('cancels when the dialog is dismissed', () => {
		const props = renderDialog()
		fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' })
		expect(props.onCancel).toHaveBeenCalled()
	})

	it('disables the confirmation while rejecting', () => {
		renderDialog({ pending: true })
		expect(
			(screen.getByText('Rejet en cours…') as HTMLButtonElement).disabled,
		).toBe(true)
	})

	it('renders nothing while closed', () => {
		renderDialog({ open: false })
		expect(screen.queryByRole('dialog')).toBeNull()
	})
})
