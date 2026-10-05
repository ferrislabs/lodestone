import { describe, expect, it } from 'vitest'

import {
	formatProblem,
	parseDefinition,
	problemsForPath,
	problemsFromError,
	problemsOutsidePath,
} from './problems'

function apiError(status: number, data: unknown): Error {
	return Object.assign(new Error('Request payload rejected'), { status, data })
}

describe('problemsFromError', () => {
	it('keeps every detail with its path', () => {
		const problems = problemsFromError(
			apiError(422, {
				message: 'Request payload rejected',
				details: [
					{ path: 'scoring.factors[2].weight', message: 'must be positive' },
					{ path: 'scoring.factors[2].weight', message: 'second' },
					{ path: 'sources', message: 'at least one source' },
				],
			}),
		)
		expect(problems).toHaveLength(3)
		expect(problems[0]).toEqual({
			path: 'scoring.factors[2].weight',
			message: 'must be positive',
		})
	})

	it('falls back to a root problem without details', () => {
		expect(problemsFromError(apiError(409, { message: 'duplicate' }))).toEqual([
			{ path: '$', message: 'Request payload rejected' },
		])
		expect(problemsFromError('boom')).toEqual([
			{ path: '$', message: 'Unexpected error' },
		])
	})
})

describe('problem helpers', () => {
	const problems = [
		{ path: 'name', message: 'taken' },
		{ path: 'llm', message: 'bad' },
	]

	it('formats a problem with its path', () => {
		expect(formatProblem(problems[1])).toBe('llm: bad')
	})

	it('splits problems around a path', () => {
		expect(problemsForPath(problems, 'name')).toEqual([problems[0]])
		expect(problemsOutsidePath(problems, 'name')).toEqual([problems[1]])
	})
})

describe('parseDefinition', () => {
	it('parses an object', () => {
		expect(parseDefinition('{"a":1}')).toEqual({ ok: true, value: { a: 1 } })
	})

	it('reports syntax errors on the root path', () => {
		const result = parseDefinition('{')
		expect(result.ok).toBe(false)
		if (!result.ok) expect(result.problems[0].path).toBe('$')
	})

	it('rejects non-objects', () => {
		const result = parseDefinition('[1]')
		expect(result).toEqual({
			ok: false,
			problems: [{ path: '$', message: 'must be a JSON object' }],
		})
	})
})
