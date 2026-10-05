import { describe, expect, it } from 'vitest'
import { canMoveTo, LEAD_STATUSES, nextStatuses } from './transitions'

const ALLOWED: ReadonlyArray<readonly [string, string]> = [
	['new', 'qualified'],
	['new', 'rejected'],
	['qualified', 'validated'],
	['qualified', 'rejected'],
	['qualified', 'new'],
	['validated', 'contacted'],
	['validated', 'rejected'],
	['rejected', 'new'],
	['new', 'uncertain'],
	['uncertain', 'new'],
	['uncertain', 'rejected'],
]

describe('lead status transitions', () => {
	it('matches the domain table for every pair', () => {
		for (const from of LEAD_STATUSES) {
			for (const to of LEAD_STATUSES) {
				const expected = ALLOWED.some(([a, b]) => a === from && b === to)
				expect(canMoveTo(from, to), `${from} -> ${to}`).toBe(expected)
			}
		}
	})

	it('treats contacted as terminal', () => {
		expect(nextStatuses('contacted')).toEqual([])
	})

	it('lists the next statuses of a qualified lead', () => {
		expect([...nextStatuses('qualified')].sort()).toEqual([
			'new',
			'rejected',
			'validated',
		])
	})
})
