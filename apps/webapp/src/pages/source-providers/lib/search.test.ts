import { describe, expect, it } from 'vitest'

import { providerSearchSchema } from '#/pages/source-providers/lib/search'

describe('provider search', () => {
	it('reads the connected flag set by the OAuth callback', () => {
		expect(providerSearchSchema.parse({ connected: 1 }).connected).toBe(true)
		expect(providerSearchSchema.parse({ connected: '1' }).connected).toBe(true)
		expect(providerSearchSchema.parse({}).connected).toBeUndefined()
		expect(providerSearchSchema.parse({ connected: 2 }).connected).toBe(
			undefined,
		)
	})

	it('keeps a short error code and drops anything else', () => {
		expect(providerSearchSchema.parse({ error: 'denied' }).error).toBe('denied')
		expect(providerSearchSchema.parse({ error: 12 }).error).toBeUndefined()
	})
})
