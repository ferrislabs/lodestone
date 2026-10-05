import { describe, expect, it } from 'vitest'

import { renderedFields, renderedVariantValues } from '#/lib/schema-form'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'

describe('french field copy', () => {
	const fields = renderedFields(PROFILE_SCHEMA, PROFILE_HINTS)

	it('walks a non-trivial set of rendered fields', () => {
		expect(fields.length).toBeGreaterThan(40)
	})

	it('has a label for every rendered property and array item', () => {
		const missing = fields
			.filter((field) => !PROFILE_HINTS.fields[field.pattern]?.label)
			.map((field) => field.pattern)
		expect(missing).toEqual([])
	})

	it('has a help text for every rendered property and array item', () => {
		const missing = fields
			.filter((field) => !PROFILE_HINTS.fields[field.pattern]?.description)
			.map((field) => field.pattern)
		expect(missing).toEqual([])
	})

	it('has no entry for a path the schema does not render', () => {
		const rendered = new Set(fields.map((field) => field.pattern))
		const stale = Object.keys(PROFILE_HINTS.fields).filter(
			(pattern) => !rendered.has(pattern),
		)
		expect(stale).toEqual([])
	})

	it('titles every union variant', () => {
		const missing = renderedVariantValues(PROFILE_SCHEMA).filter(
			(value) => !PROFILE_HINTS.variants[value],
		)
		expect(missing).toEqual([])
	})
})
