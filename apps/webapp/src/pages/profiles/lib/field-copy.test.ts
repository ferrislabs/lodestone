import { describe, expect, it } from 'vitest'

import { renderedFields, renderedVariantValues } from '#/lib/schema-form'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import { tabOfPath } from '#/pages/profiles/lib/tabs'

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

	it('names the enrichment fields shown on the Enrichissement tab', () => {
		const enrichment = fields
			.map((field) => field.pattern)
			.filter(
				(pattern) =>
					pattern.startsWith('enrichment.') || pattern.startsWith('inspection'),
			)
		expect(enrichment).toEqual(
			expect.arrayContaining([
				'inspection.enabled',
				'enrichment.custom.fingerprints',
				'enrichment.custom.extra_subdomains',
				'enrichment.custom.excluded_products',
				'enrichment.highlight_keywords',
			]),
		)
		for (const pattern of enrichment) {
			expect(tabOfPath(pattern)).toBe('enrichment')
		}
		const copy = PROFILE_HINTS.fields
		expect(copy['enrichment.custom.extra_subdomains']?.label).toBe(
			'Sous-domaines supplémentaires',
		)
		expect(copy['enrichment.custom.excluded_products']?.label).toBe(
			'Produits exclus',
		)
		expect(copy['enrichment.highlight_keywords']?.label).toBe(
			'Mots-clés à mettre en avant',
		)
		expect(copy['enrichment.highlight_keywords']?.description).toContain(
			'Vide : les mots-clés des agents de recherche',
		)
		expect(copy['enrichment.custom.fingerprints[]']?.add).toBe(
			'Ajouter une empreinte',
		)
	})

	it('titles every union variant', () => {
		const missing = renderedVariantValues(PROFILE_SCHEMA).filter(
			(value) => !PROFILE_HINTS.variants[value],
		)
		expect(missing).toEqual([])
	})
})
