import { describe, expect, it } from 'vitest'

import { mergeTags, splitTags } from '#/lib/tags'

describe('tags', () => {
	it('splits on commas, semicolons and newlines and trims', () => {
		expect(splitTags(' a, b;c\nd ,, ')).toEqual(['a', 'b', 'c', 'd'])
	})

	it('ignores empty and case-insensitive duplicate entries', () => {
		expect(mergeTags(['Auth'], ['auth', ' ', 'sso', 'SSO'])).toEqual({
			next: ['Auth', 'sso'],
			added: 1,
			duplicates: 2,
		})
	})
})
