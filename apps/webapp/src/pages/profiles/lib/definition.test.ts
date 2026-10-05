import { describe, expect, it } from 'vitest'

import {
	exportFilename,
	NEW_PROFILE_TEMPLATE,
	prettyDefinition,
	saveResultMessage,
} from './definition'

describe('saveResultMessage', () => {
	it('announces a created revision', () => {
		expect(saveResultMessage(2, 3)).toBe('Revision 3 created')
	})

	it('announces an unchanged definition', () => {
		expect(saveResultMessage(3, 3)).toBe('No change, revision 3 kept')
	})
})

describe('definition text', () => {
	it('pretty prints with two spaces', () => {
		expect(prettyDefinition({ a: { b: 1 } })).toBe(
			'{\n  "a": {\n    "b": 1\n  }\n}',
		)
	})

	it('ships a template that is valid JSON', () => {
		expect(() => JSON.parse(NEW_PROFILE_TEMPLATE)).not.toThrow()
	})

	it('builds a safe export filename', () => {
		expect(exportFilename('IAM / FerrisKey')).toBe('IAM___FerrisKey.json')
	})
})
