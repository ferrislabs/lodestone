import { describe, expect, it } from 'vitest'

import { draftStatusLabel } from './labels'

describe('draftStatusLabel', () => {
	it('labels draft statuses in french', () => {
		expect(draftStatusLabel('pending')).toBe('En attente')
		expect(draftStatusLabel('approved')).toBe('Approuvé')
		expect(draftStatusLabel('rejected')).toBe('Rejeté')
		expect(draftStatusLabel('other')).toBe('other')
	})
})
