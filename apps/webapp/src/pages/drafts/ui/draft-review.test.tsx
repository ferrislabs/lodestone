import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftEditor } from './draft-editor'
import { DraftReview } from './draft-review'

afterEach(cleanup)

const entry: DraftEntry = {
	assessment_id: 'a1',
	lead: {
		id: 'l1',
		domain: 'acme.test',
		score: 42,
		confidence: 66,
		verdict: 'a_verifier',
		profile: { id: 'p1', name: 'Fintech' },
	},
	draft: { subject: 'Hello Acme', body: 'Body text', status: 'pending' },
	summary: 'Summary text',
	verdict_narrative: 'Narrative text',
	approach_angle: 'Angle text',
	evidence: [{ quote: 'We are hiring', provenance: 'https://acme.test/jobs' }],
	evidence_verified: true,
	contact: { address: 'cto@acme.test', method: 'scraped', confidence: 0.9 },
	created_at: '2026-02-01T00:00:00Z',
}

function renderReview(
	overrides: Partial<Parameters<typeof DraftReview>[0]> = {},
) {
	const props = {
		state: 'ready' as const,
		errorMessage: null,
		entry,
		editor: <div>editor</div>,
		rejectForm: <div>reject-form</div>,
		rejectOpen: false,
		dirty: false,
		deciding: false,
		decisionError: null,
		onBack: vi.fn(),
		onOpenLead: vi.fn(),
		onApprove: vi.fn(),
		onStartReject: vi.fn(),
		onCopy: vi.fn(),
		...overrides,
	}
	render(<DraftReview {...props} />)
	return props
}

describe('DraftReview suggestion', () => {
	const suggested: DraftEntry = {
		...entry,
		draft: {
			...entry.draft,
			suggested_subject: 'Sujet proposé',
			suggested_body: 'Texte proposé',
			outdated_at: '2026-02-02T00:00:00Z',
		},
	}

	it('says the summary changed after the edit and shows the suggestion read-only', () => {
		renderReview({ entry: suggested })
		expect(
			screen.getByText(/Le bilan a été mis à jour après votre modification/),
		).toBeTruthy()
		expect(screen.getByText('Sujet proposé')).toBeTruthy()
		expect(screen.getByText('Texte proposé')).toBeTruthy()
		expect(screen.queryAllByRole('textbox')).toHaveLength(0)
	})

	it('copies the suggested subject and body', () => {
		const props = renderReview({ entry: suggested })
		fireEvent.click(screen.getByRole('button', { name: "Copier l'objet" }))
		expect(props.onCopy).toHaveBeenLastCalledWith('Sujet proposé')
		fireEvent.click(screen.getByRole('button', { name: 'Copier le corps' }))
		expect(props.onCopy).toHaveBeenLastCalledWith('Texte proposé')
	})

	it('shows nothing when no suggestion was made', () => {
		renderReview()
		expect(screen.queryByText('Nouvelle suggestion du modèle')).toBeNull()
	})
})

describe('DraftReview', () => {
	it('shows the lead context, evidence link and contact', () => {
		renderReview()
		expect(screen.getByText('acme.test')).toBeTruthy()
		expect(screen.getByText('Angle text')).toBeTruthy()
		expect(screen.getByText('We are hiring')).toBeTruthy()
		expect(
			screen
				.getByText('https://acme.test/jobs')
				.closest('a')
				?.getAttribute('href'),
		).toBe('https://acme.test/jobs')
		expect(screen.getByText(/cto@acme.test/)).toBeTruthy()
	})

	it('opens the lead', () => {
		const props = renderReview()
		fireEvent.click(screen.getByText('acme.test'))
		expect(props.onOpenLead).toHaveBeenCalledWith('l1')
	})

	it('hides the original text when the draft was not edited', () => {
		renderReview()
		expect(screen.queryByText('Texte original du modèle')).toBeNull()
	})

	it('shows the original text when the draft was edited', () => {
		renderReview({
			entry: {
				...entry,
				draft: {
					...entry.draft,
					original_subject: 'Model subject',
					original_body: 'Model body',
				},
			},
		})
		expect(screen.getByText('Texte original du modèle')).toBeTruthy()
		expect(screen.getByText('Model subject')).toBeTruthy()
		expect(screen.getByText('Model body')).toBeTruthy()
	})

	it('states that approving sends nothing and approves on click', () => {
		const props = renderReview()
		fireEvent.click(
			screen.getByText("Approuver (marque comme prêt, rien n'est envoyé)"),
		)
		expect(props.onApprove).toHaveBeenCalled()
	})

	it('blocks approval while there are unsaved changes', () => {
		renderReview({ dirty: true })
		const button = screen.getByText(
			"Approuver (marque comme prêt, rien n'est envoyé)",
		)
		expect((button as HTMLButtonElement).disabled).toBe(true)
	})

	it('shows the server message of a failed decision', () => {
		renderReview({ decisionError: 'Draft already decided' })
		expect(screen.getByRole('alert').textContent).toBe('Draft already decided')
	})

	it('opens the rejection dialog', () => {
		const props = renderReview()
		fireEvent.click(screen.getByText('Rejeter'))
		expect(props.onStartReject).toHaveBeenCalled()
		cleanup()
		renderReview({ rejectOpen: true })
		expect(screen.getByText('reject-form')).toBeTruthy()
		expect((screen.getByText('Rejeter') as HTMLButtonElement).disabled).toBe(
			true,
		)
	})

	it('offers no decision for an already decided draft', () => {
		renderReview({
			entry: { ...entry, draft: { ...entry.draft, status: 'approved' } },
		})
		expect(screen.queryByText('Rejeter')).toBeNull()
		expect(screen.getByText('Ce brouillon a déjà été approuvé.')).toBeTruthy()
	})
})

describe('DraftReview decided draft', () => {
	const decidedDraft = {
		subject: 'Hello Acme',
		body: 'Line one\nLine two',
	}

	it.each([
		['approved', 'approuvé'],
		['rejected', 'rejeté'],
	] as const)('shows a %s draft read-only with the actions', (status, label) => {
		const props = renderReview({
			entry: { ...entry, draft: { ...decidedDraft, status } },
		})
		expect(screen.getByText(`Ce brouillon a déjà été ${label}.`)).toBeTruthy()
		expect(screen.getByText('Hello Acme')).toBeTruthy()
		expect(
			screen.getByText(
				(_, element) =>
					element?.tagName === 'P' &&
					element.textContent === 'Line one\nLine two' &&
					element.className.includes('whitespace-pre-wrap'),
			),
		).toBeTruthy()
		expect(screen.queryAllByRole('textbox')).toHaveLength(0)
		expect(screen.queryByText('editor')).toBeNull()
		expect(screen.queryByText('Rejeter')).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: 'Copier le brouillon' }))
		expect(props.onCopy).toHaveBeenLastCalledWith(
			'Hello Acme\n\nLine one\nLine two',
		)
		fireEvent.click(screen.getByRole('button', { name: "Copier l'objet" }))
		expect(props.onCopy).toHaveBeenLastCalledWith('Hello Acme')
		fireEvent.click(screen.getByRole('button', { name: 'Copier le corps' }))
		expect(props.onCopy).toHaveBeenLastCalledWith('Line one\nLine two')
	})

	it('links a decided draft to the mail client with the best contact', () => {
		renderReview({
			entry: { ...entry, draft: { ...decidedDraft, status: 'approved' } },
		})
		expect(
			screen
				.getByRole('link', { name: 'Ouvrir dans la messagerie' })
				.getAttribute('href'),
		).toBe(
			'mailto:cto@acme.test?subject=Hello%20Acme&body=Line%20one%0D%0ALine%20two',
		)
	})

	it('has no mail link for a decided draft without a contact', () => {
		renderReview({
			entry: {
				...entry,
				contact: null,
				draft: { ...decidedDraft, status: 'rejected' },
			},
		})
		expect(screen.queryByRole('link', { name: /messagerie/ })).toBeNull()
		expect(screen.getByRole('button', { name: 'Copier le corps' })).toBeTruthy()
	})

	it('shows no copy actions of its own for a pending draft but the editor slot', () => {
		renderReview()
		expect(screen.getByText('editor')).toBeTruthy()
		expect(screen.queryByRole('button', { name: 'Copier le corps' })).toBeNull()
	})
})

describe('DraftEditor', () => {
	const values = { subject: 'S', body: 'B' }

	it('shows each error on its own field', () => {
		render(
			<DraftEditor
				values={values}
				contactAddress={null}
				onCopy={() => {}}
				fieldErrors={{ subject: 'subject too long', body: 'body empty' }}
				formError={null}
				dirty
				saving={false}
				onChange={() => {}}
				onSave={() => {}}
			/>,
		)
		const subject = screen.getByLabelText('Objet')
		const body = screen.getByLabelText('Corps')
		expect(subject.getAttribute('aria-invalid')).toBe('true')
		expect(
			document.getElementById(subject.getAttribute('aria-describedby') ?? '')
				?.textContent,
		).toBe('subject too long')
		expect(
			document.getElementById(body.getAttribute('aria-describedby') ?? '')
				?.textContent,
		).toBe('body empty')
	})

	it('flags only the faulty field', () => {
		render(
			<DraftEditor
				values={values}
				contactAddress={null}
				onCopy={() => {}}
				fieldErrors={{ body: 'body empty' }}
				formError={null}
				dirty={false}
				saving={false}
				onChange={() => {}}
				onSave={() => {}}
			/>,
		)
		expect(screen.getByLabelText('Objet').getAttribute('aria-invalid')).toBe(
			'false',
		)
		expect(screen.getByLabelText('Corps').getAttribute('aria-invalid')).toBe(
			'true',
		)
	})

	it('shows the dirty indicator and reports edits and save', () => {
		const onChange = vi.fn()
		const onSave = vi.fn()
		render(
			<DraftEditor
				values={values}
				contactAddress={null}
				onCopy={() => {}}
				fieldErrors={{}}
				formError={null}
				dirty
				saving={false}
				onChange={onChange}
				onSave={onSave}
			/>,
		)
		expect(screen.getByText('Modifications non enregistrées')).toBeTruthy()
		fireEvent.change(screen.getByLabelText('Objet'), {
			target: { value: 'New' },
		})
		expect(onChange).toHaveBeenCalledWith('subject', 'New')
		fireEvent.click(screen.getByText('Enregistrer les modifications'))
		expect(onSave).toHaveBeenCalled()
	})

	it('copies the current text of the fields and links the contact', () => {
		const onCopy = vi.fn()
		render(
			<DraftEditor
				values={{ subject: 'Edited subject', body: 'Edited body' }}
				fieldErrors={{}}
				formError={null}
				dirty
				saving={false}
				contactAddress="cto@acme.test"
				onChange={() => {}}
				onSave={() => {}}
				onCopy={onCopy}
			/>,
		)
		fireEvent.click(screen.getByRole('button', { name: "Copier l'objet" }))
		expect(onCopy).toHaveBeenLastCalledWith('Edited subject')
		fireEvent.click(screen.getByRole('button', { name: 'Copier le corps' }))
		expect(onCopy).toHaveBeenLastCalledWith('Edited body')
		fireEvent.click(screen.getByRole('button', { name: 'Copier le brouillon' }))
		expect(onCopy).toHaveBeenLastCalledWith('Edited subject\n\nEdited body')
		expect(
			screen
				.getByRole('link', { name: 'Ouvrir dans la messagerie' })
				.getAttribute('href'),
		).toBe('mailto:cto@acme.test?subject=Edited%20subject&body=Edited%20body')
	})

	it('does not submit the form when a copy action is used', () => {
		const onSave = vi.fn()
		render(
			<DraftEditor
				values={values}
				fieldErrors={{}}
				formError={null}
				dirty
				saving={false}
				contactAddress={null}
				onChange={() => {}}
				onSave={onSave}
				onCopy={() => {}}
			/>,
		)
		fireEvent.click(screen.getByRole('button', { name: 'Copier le corps' }))
		expect(onSave).not.toHaveBeenCalled()
	})

	it('disables saving when clean', () => {
		render(
			<DraftEditor
				values={values}
				contactAddress={null}
				onCopy={() => {}}
				fieldErrors={{}}
				formError={null}
				dirty={false}
				saving={false}
				onChange={() => {}}
				onSave={() => {}}
			/>,
		)
		expect(
			(screen.getByText('Enregistrer les modifications') as HTMLButtonElement)
				.disabled,
		).toBe(true)
		expect(screen.queryByText('Modifications non enregistrées')).toBeNull()
	})
})
