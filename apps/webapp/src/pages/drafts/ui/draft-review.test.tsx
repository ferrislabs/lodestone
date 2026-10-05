import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftEditor } from './draft-editor'
import { DraftReview } from './draft-review'
import { RejectForm } from './reject-form'

afterEach(cleanup)

const entry: DraftEntry = {
	assessment_id: 'a1',
	lead: {
		id: 'l1',
		domain: 'acme.test',
		score: 42,
		profile: { id: 'p1', name: 'Fintech' },
	},
	draft: { subject: 'Hello Acme', body: 'Body text', status: 'pending' },
	summary: 'Summary text',
	approach_angle: 'Angle text',
	evidence: [{ quote: 'We are hiring', provenance: 'https://acme.test/jobs' }],
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
		...overrides,
	}
	render(<DraftReview {...props} />)
	return props
}

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
		expect(screen.queryByText('Original text from the model')).toBeNull()
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
		expect(screen.getByText('Original text from the model')).toBeTruthy()
		expect(screen.getByText('Model subject')).toBeTruthy()
		expect(screen.getByText('Model body')).toBeTruthy()
	})

	it('states that approving sends nothing and approves on click', () => {
		const props = renderReview()
		fireEvent.click(screen.getByText('Approve (marks ready, nothing is sent)'))
		expect(props.onApprove).toHaveBeenCalled()
	})

	it('blocks approval while there are unsaved changes', () => {
		renderReview({ dirty: true })
		const button = screen.getByText('Approve (marks ready, nothing is sent)')
		expect((button as HTMLButtonElement).disabled).toBe(true)
	})

	it('shows the server message of a failed decision', () => {
		renderReview({ decisionError: 'Draft already decided' })
		expect(screen.getByRole('alert').textContent).toBe('Draft already decided')
	})

	it('opens the reject form', () => {
		const props = renderReview()
		fireEvent.click(screen.getByText('Reject'))
		expect(props.onStartReject).toHaveBeenCalled()
		cleanup()
		renderReview({ rejectOpen: true })
		expect(screen.getByText('reject-form')).toBeTruthy()
	})

	it('offers no decision for an already decided draft', () => {
		renderReview({
			entry: { ...entry, draft: { ...entry.draft, status: 'approved' } },
		})
		expect(screen.queryByText('Reject')).toBeNull()
		expect(screen.getByText('This draft was already approved.')).toBeTruthy()
	})
})

describe('DraftEditor', () => {
	const values = { subject: 'S', body: 'B' }

	it('shows each error on its own field', () => {
		render(
			<DraftEditor
				values={values}
				fieldErrors={{ subject: 'subject too long', body: 'body empty' }}
				formError={null}
				dirty
				saving={false}
				onChange={() => {}}
				onSave={() => {}}
			/>,
		)
		const subject = screen.getByLabelText('Subject')
		const body = screen.getByLabelText('Body')
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
				fieldErrors={{ body: 'body empty' }}
				formError={null}
				dirty={false}
				saving={false}
				onChange={() => {}}
				onSave={() => {}}
			/>,
		)
		expect(screen.getByLabelText('Subject').getAttribute('aria-invalid')).toBe(
			'false',
		)
		expect(screen.getByLabelText('Body').getAttribute('aria-invalid')).toBe(
			'true',
		)
	})

	it('shows the dirty indicator and reports edits and save', () => {
		const onChange = vi.fn()
		const onSave = vi.fn()
		render(
			<DraftEditor
				values={values}
				fieldErrors={{}}
				formError={null}
				dirty
				saving={false}
				onChange={onChange}
				onSave={onSave}
			/>,
		)
		expect(screen.getByText('Unsaved changes')).toBeTruthy()
		fireEvent.change(screen.getByLabelText('Subject'), {
			target: { value: 'New' },
		})
		expect(onChange).toHaveBeenCalledWith('subject', 'New')
		fireEvent.click(screen.getByText('Save changes'))
		expect(onSave).toHaveBeenCalled()
	})

	it('disables saving when clean', () => {
		render(
			<DraftEditor
				values={values}
				fieldErrors={{}}
				formError={null}
				dirty={false}
				saving={false}
				onChange={() => {}}
				onSave={() => {}}
			/>,
		)
		expect(
			(screen.getByText('Save changes') as HTMLButtonElement).disabled,
		).toBe(true)
		expect(screen.queryByText('Unsaved changes')).toBeNull()
	})
})

describe('RejectForm', () => {
	it('shows the reason error on the reason field', () => {
		render(
			<RejectForm
				reason="x"
				reasonError="too long"
				formError={null}
				rejecting={false}
				onReasonChange={() => {}}
				onConfirm={() => {}}
				onCancel={() => {}}
			/>,
		)
		const reason = screen.getByLabelText('Reason (optional)')
		expect(reason.getAttribute('aria-invalid')).toBe('true')
		expect(
			document.getElementById(reason.getAttribute('aria-describedby') ?? '')
				?.textContent,
		).toBe('too long')
	})

	it('confirms and cancels', () => {
		const onConfirm = vi.fn()
		const onCancel = vi.fn()
		render(
			<RejectForm
				reason=""
				reasonError={null}
				formError="Draft already decided"
				rejecting={false}
				onReasonChange={() => {}}
				onConfirm={onConfirm}
				onCancel={onCancel}
			/>,
		)
		expect(screen.getByText('Draft already decided')).toBeTruthy()
		fireEvent.click(screen.getByText('Confirm rejection'))
		expect(onConfirm).toHaveBeenCalled()
		fireEvent.click(screen.getByText('Cancel'))
		expect(onCancel).toHaveBeenCalled()
	})
})
