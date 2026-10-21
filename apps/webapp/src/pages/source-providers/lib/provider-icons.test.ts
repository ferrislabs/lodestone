import { Database } from 'lucide-react'
import { describe, expect, it } from 'vitest'

import { summary } from '#/pages/source-providers/lib/fixtures'
import {
	CATALOG_KEYS,
	providerIcon,
} from '#/pages/source-providers/lib/provider-icons'

describe('providerIcon', () => {
	it('has an icon for every catalog key', () => {
		for (const key of CATALOG_KEYS) {
			const icon = providerIcon(key)
			expect(icon).toBeTruthy()
			if (icon.kind === 'logo') expect(icon.src.length).toBeGreaterThan(0)
			else expect(icon.Icon).toBeTruthy()
		}
	})

	it('has an icon for the keys of the fixtures', () => {
		expect(providerIcon(summary().key)).not.toEqual(providerIcon('unknown'))
		expect(providerIcon('france_travail').kind).toBe('logo')
	})

	it('uses real logos where an official asset exists', () => {
		for (const key of [
			'boamp',
			'ted',
			'decp',
			'france_travail',
			'hacker_news_hiring',
			'company_research',
		]) {
			expect(providerIcon(key).kind).toBe('logo')
		}
	})

	it('falls back to a lucide icon', () => {
		expect(providerIcon('http_probe').kind).toBe('lucide')
		expect(providerIcon('certificate_transparency').kind).toBe('lucide')
	})

	it('falls back to the database icon for an unknown key', () => {
		expect(providerIcon('nope')).toEqual({ kind: 'lucide', Icon: Database })
	})
})
