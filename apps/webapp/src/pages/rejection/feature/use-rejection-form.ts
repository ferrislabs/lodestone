import { useState } from 'react'

import {
	EMPTY_REJECTION,
	isRejectionValid,
	type RejectionDraft,
	type RejectionErrors,
	type RejectionPayload,
	type RejectionReason,
	rejectionPayload,
	validateRejection,
} from '#/pages/rejection/lib/reasons'

const NO_ERRORS: RejectionErrors = { reason: null, note: null }

export function useRejectionForm() {
	const [open, setOpen] = useState(false)
	const [draft, setDraft] = useState<RejectionDraft>(EMPTY_REJECTION)
	const [submitted, setSubmitted] = useState(false)

	return {
		open,
		reason: draft.reason,
		note: draft.note,
		errors: submitted ? validateRejection(draft) : NO_ERRORS,
		start: () => setOpen(true),
		close: () => {
			setOpen(false)
			setDraft(EMPTY_REJECTION)
			setSubmitted(false)
		},
		setReason: (reason: RejectionReason) =>
			setDraft((current) => ({ ...current, reason })),
		setNote: (note: string) => setDraft((current) => ({ ...current, note })),
		submit: (send: (payload: RejectionPayload) => void) => {
			setSubmitted(true)
			if (!isRejectionValid(validateRejection(draft))) return
			const payload = rejectionPayload(draft)
			if (payload) send(payload)
		},
	}
}
