import { describe, expect, it } from 'vitest'

import { agreementLabel, escalationLabel } from '#/pages/leads/lib/agreement'

describe('agreementLabel', () => {
	it('shows a fraction of the samples', () => {
		expect(agreementLabel(1, 3)).toBe('Accord : 3/3')
		expect(agreementLabel(2 / 3, 3)).toBe('Accord : 2/3')
		expect(agreementLabel(0.5, 2)).toBe('Accord : 1/2')
		expect(agreementLabel(0.6, 5)).toBe('Accord : 3/5')
	})

	it('falls back to a percentage when the share is not a fraction of the samples', () => {
		expect(agreementLabel(0.4167, 3)).toBe('Accord : 41,7 %')
		expect(agreementLabel(0.75, 3)).toBe('Accord : 75 %')
	})

	it('shows nothing without an agreement', () => {
		expect(agreementLabel(null, 3)).toBeNull()
		expect(agreementLabel(undefined, 3)).toBeNull()
	})
})

describe('escalationLabel', () => {
	it('names the provider, then the model', () => {
		expect(escalationLabel('Claude', 'claude-x')).toBe('Escaladé vers Claude')
		expect(escalationLabel(null, 'claude-x')).toBe('Escaladé vers claude-x')
		expect(escalationLabel(null, '')).toBe('Escaladé')
	})
})
