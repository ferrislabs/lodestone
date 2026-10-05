import { describe, expect, it } from 'vitest'

import {
	deleteConflictMessage,
	keyLabel,
	NO_PROVIDER,
	selectionFromAssignments,
	tierDiff,
} from './tiers'

describe('selectionFromAssignments', () => {
	it('defaults to no provider and reads assignments', () => {
		expect(
			selectionFromAssignments([
				{ tier: 'triage', provider_id: 'a' },
				{ tier: 'qualification', provider_id: null },
			]),
		).toEqual({ triage: 'a', qualification: NO_PROVIDER })
		expect(selectionFromAssignments([])).toEqual({
			triage: NO_PROVIDER,
			qualification: NO_PROVIDER,
		})
	})
})

describe('tierDiff', () => {
	it('returns nothing when nothing changed', () => {
		const same = { triage: 'a', qualification: NO_PROVIDER }
		expect(tierDiff(same, same)).toEqual([])
	})

	it('assigns, reassigns and unassigns', () => {
		expect(
			tierDiff(
				{ triage: 'a', qualification: 'b' },
				{ triage: 'c', qualification: NO_PROVIDER },
			),
		).toEqual([
			{ tier: 'triage', op: 'assign', providerId: 'c' },
			{ tier: 'qualification', op: 'unassign' },
		])
	})

	it('does not unassign a tier that was already empty', () => {
		expect(
			tierDiff(
				{ triage: NO_PROVIDER, qualification: 'b' },
				{ triage: NO_PROVIDER, qualification: 'b' },
			),
		).toEqual([])
	})
})

describe('labels', () => {
	it('names the assigned tiers in the conflict message', () => {
		expect(deleteConflictMessage(['triage'])).toBe(
			'Cannot delete: assigned to the triage tier',
		)
	})

	it('labels the key state', () => {
		expect(keyLabel(true)).toBe('Key set')
		expect(keyLabel(false)).toBe('No key')
	})
})
