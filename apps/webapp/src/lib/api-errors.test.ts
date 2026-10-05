import { describe, expect, it } from 'vitest'

import { parseApiError } from './api-errors'

function apiError(status: number, data: unknown): Error {
	return Object.assign(new Error('Request payload rejected'), { status, data })
}

describe('parseApiError', () => {
	it('maps detail paths to field messages', () => {
		const parsed = parseApiError(
			apiError(422, {
				code: 'E_UNPROCESSABLE_ENTITY',
				message: 'Request payload rejected',
				status: 422,
				details: [
					{ path: 'subject', message: 'must be at most 200 characters' },
					{ path: 'body', message: 'must not be empty' },
				],
			}),
		)
		expect(parsed.fields).toEqual({
			subject: 'must be at most 200 characters',
			body: 'must not be empty',
		})
		expect(parsed.status).toBe(422)
		expect(parsed.message).toBe('Request payload rejected')
	})

	it('keeps the first message of a repeated path', () => {
		const parsed = parseApiError(
			apiError(422, {
				code: 'x',
				message: 'm',
				status: 422,
				details: [
					{ path: 'body', message: 'first' },
					{ path: 'body', message: 'second' },
				],
			}),
		)
		expect(parsed.fields).toEqual({ body: 'first' })
	})

	it('returns no fields for a conflict', () => {
		const parsed = parseApiError(
			apiError(409, {
				code: 'E_CONFLICT',
				message: 'Already decided',
				status: 409,
			}),
		)
		expect(parsed.fields).toEqual({})
		expect(parsed.message).toBe('Already decided')
		expect(parsed.status).toBe(409)
	})

	it('falls back to the error message when the body is not an error body', () => {
		const parsed = parseApiError(apiError(500, { oops: true }))
		expect(parsed.fields).toEqual({})
		expect(parsed.message).toBe('Request payload rejected')
	})

	it('handles non-error values', () => {
		expect(parseApiError('boom')).toEqual({
			message: 'Unexpected error',
			status: null,
			fields: {},
		})
	})
})
