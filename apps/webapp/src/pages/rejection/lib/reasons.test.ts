import { describe, expect, it } from 'vitest'

import {
	EMPTY_REJECTION,
	isRejectionValid,
	REJECTION_NOTE_MAX,
	REJECTION_REASONS,
	rejectionPayload,
	rejectionReasonLabel,
	validateRejection,
} from './reasons'

describe('rejection reasons', () => {
	it('labels every reason in French and falls back to the raw value', () => {
		expect(REJECTION_REASONS.map(rejectionReasonLabel)).toEqual([
			'Hors cible',
			'Déjà client',
			'Concurrent',
			'Éditeur ou intégrateur du domaine',
			'Hébergeur ou agence (le signal concerne un client)',
			'Mauvaise donnée',
			'Autre',
		])
		expect(rejectionReasonLabel('inconnue')).toBe('inconnue')
	})

	it('requires a reason', () => {
		const errors = validateRejection(EMPTY_REJECTION)
		expect(errors.reason).toBe('Choisissez une raison de rejet')
		expect(isRejectionValid(errors)).toBe(false)
	})

	it('requires a note for autre only', () => {
		expect(validateRejection({ reason: 'autre', note: '   ' }).note).toBe(
			'Précisez la raison quand elle est « Autre »',
		)
		expect(
			isRejectionValid(validateRejection({ reason: 'hors_cible', note: '' })),
		).toBe(true)
		expect(
			isRejectionValid(validateRejection({ reason: 'autre', note: 'détail' })),
		).toBe(true)
	})

	it('bounds the note', () => {
		const long = 'n'.repeat(REJECTION_NOTE_MAX + 1)
		expect(validateRejection({ reason: 'concurrent', note: long }).note).toBe(
			'Au maximum 500 caractères',
		)
		expect(
			validateRejection({
				reason: 'concurrent',
				note: 'n'.repeat(REJECTION_NOTE_MAX),
			}).note,
		).toBeNull()
	})

	it('builds the payload with a trimmed optional note', () => {
		expect(rejectionPayload(EMPTY_REJECTION)).toBeNull()
		expect(rejectionPayload({ reason: 'hors_cible', note: '  ' })).toEqual({
			reason: 'hors_cible',
		})
		expect(rejectionPayload({ reason: 'autre', note: '  détail ' })).toEqual({
			reason: 'autre',
			note: 'détail',
		})
	})
})
