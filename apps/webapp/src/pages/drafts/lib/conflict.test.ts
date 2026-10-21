import { describe, expect, it } from 'vitest'

import { parseApiError } from '#/lib/api-errors'
import { CONFLICT_MESSAGE, isConflict } from './conflict'

function apiError(status: number, message: string): Error {
	return Object.assign(new Error(message), { status, data: { message } })
}

describe('CONFLICT_MESSAGE', () => {
	it('asks the operator to check the reloaded draft and retry', () => {
		expect(CONFLICT_MESSAGE).toBe(
			"Ce brouillon a changé depuis son chargement. La dernière version vient d'être rechargée : vérifiez-la, puis recommencez.",
		)
	})
})

describe('isConflict', () => {
	it('is true for a 409 answer', () => {
		expect(
			isConflict(parseApiError(apiError(409, 'Conflict: stale revision'))),
		).toBe(true)
	})

	it.each([400, 404, 422, 500])('is false for a %i answer', (status) => {
		expect(isConflict(parseApiError(apiError(status, 'Something')))).toBe(false)
	})

	it('is false for an error without a status', () => {
		expect(isConflict(parseApiError(new Error('Failed to fetch')))).toBe(false)
		expect(isConflict(parseApiError('boom'))).toBe(false)
	})
})
