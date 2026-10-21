import { describe, expect, it } from 'vitest'

import { isEdited, previewText } from './edit'

const base = {
	subject: 's',
	body: 'b',
	status: 'pending' as const,
	revision: 0,
}

describe('draft edit helpers', () => {
	it('is not edited without original text', () => {
		expect(isEdited(base)).toBe(false)
		expect(
			isEdited({ ...base, original_subject: null, original_body: null }),
		).toBe(false)
	})

	it('is edited when an original is kept', () => {
		expect(isEdited({ ...base, original_subject: 'x' })).toBe(true)
		expect(isEdited({ ...base, original_body: 'x' })).toBe(true)
	})

	it('flattens and truncates previews', () => {
		expect(previewText('a  b\n c', 20)).toBe('a b c')
		expect(previewText('abcdefghij', 5)).toBe('abcd…')
	})
})
