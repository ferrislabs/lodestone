import { describe, expect, it } from 'vitest'

import { validateValue } from '#/lib/schema-form'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'

import {
	exportFilename,
	NEW_PROFILE_TEMPLATE,
	prettyDefinition,
	saveResultMessage,
} from './definition'

describe('saveResultMessage', () => {
	it('announces a created revision', () => {
		expect(saveResultMessage(2, 3)).toBe('Révision 3 créée')
	})

	it('announces an unchanged definition', () => {
		expect(saveResultMessage(3, 3)).toBe(
			'Aucun changement, révision 3 conservée',
		)
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

	it('ships a template at the current version that fits the bundled schema', () => {
		const template = JSON.parse(NEW_PROFILE_TEMPLATE)
		expect(template.schema_version).toBe(16)
		const properties = Object.keys(PROFILE_SCHEMA.properties ?? {})
		for (const key of Object.keys(template)) expect(properties).toContain(key)
		for (const key of PROFILE_SCHEMA.required ?? [])
			expect(template).toHaveProperty(key)
		const structural = validateValue(
			PROFILE_SCHEMA,
			PROFILE_SCHEMA,
			template,
		).filter((problem) => problem.message !== 'must not be empty')
		expect(structural).toEqual([])
	})

	it('builds a safe export filename', () => {
		expect(exportFilename('IAM / FerrisKey')).toBe('IAM___FerrisKey.json')
	})
})
