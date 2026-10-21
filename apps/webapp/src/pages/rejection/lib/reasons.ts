import type { Schemas } from '#/api/api.client'

export type RejectionReason = Schemas.RejectionReasonDto

export const REJECTION_NOTE_MAX = 500

export const REJECTION_REASONS: readonly RejectionReason[] = [
	'hors_cible',
	'deja_client',
	'concurrent',
	'editeur_integrateur',
	'hebergeur_agence',
	'mauvaise_donnee',
	'autre',
]

const LABELS: Readonly<Record<RejectionReason, string>> = {
	hors_cible: 'Hors cible',
	deja_client: 'Déjà client',
	concurrent: 'Concurrent',
	editeur_integrateur: 'Éditeur ou intégrateur du domaine',
	hebergeur_agence: 'Hébergeur ou agence (le signal concerne un client)',
	mauvaise_donnee: 'Mauvaise donnée',
	autre: 'Autre',
}

export function rejectionReasonLabel(reason: string): string {
	return LABELS[reason as RejectionReason] ?? reason
}

export interface RejectionDraft {
	reason: RejectionReason | null
	note: string
}

export const EMPTY_REJECTION: RejectionDraft = { reason: null, note: '' }

export interface RejectionErrors {
	reason: string | null
	note: string | null
}

export function validateRejection(draft: RejectionDraft): RejectionErrors {
	const note = draft.note.trim()
	return {
		reason: draft.reason === null ? 'Choisissez une raison de rejet' : null,
		note:
			draft.reason === 'autre' && note === ''
				? 'Précisez la raison quand elle est « Autre »'
				: note.length > REJECTION_NOTE_MAX
					? `Au maximum ${REJECTION_NOTE_MAX} caractères`
					: null,
	}
}

export function isRejectionValid(errors: RejectionErrors): boolean {
	return errors.reason === null && errors.note === null
}

export interface RejectionPayload {
	reason: RejectionReason
	note?: string
}

export function rejectionPayload(
	draft: RejectionDraft,
): RejectionPayload | null {
	if (draft.reason === null) return null
	const note = draft.note.trim()
	return note === '' ? { reason: draft.reason } : { reason: draft.reason, note }
}
