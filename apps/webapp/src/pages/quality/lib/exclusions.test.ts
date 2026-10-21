import { describe, expect, it } from 'vitest'

import {
	clipboardText,
	exclusionListName,
	exclusionOptionLabel,
} from '#/pages/quality/lib/exclusions'

describe('exclusion helpers', () => {
	it('copies the legal name and the domain, or the domain alone', () => {
		expect(clipboardText({ domain: 'acme.test', legal_name: 'ACME SAS' })).toBe(
			'ACME SAS, acme.test',
		)
		expect(clipboardText({ domain: 'acme.test', legal_name: null })).toBe(
			'acme.test',
		)
		expect(clipboardText({ domain: 'acme.test', legal_name: '  ' })).toBe(
			'acme.test',
		)
	})

	it('maps each rejection reason to a list of the exclusions prompt', () => {
		expect(exclusionListName('deja_client')).toBe('Clients actuels')
		expect(exclusionListName('concurrent')).toBe('Concurrents')
		expect(exclusionListName('editeur_integrateur')).toBe(
			'Éditeurs et intégrateurs du domaine',
		)
		expect(exclusionListName('hebergeur_agence')).toBe(
			'Éditeurs et intégrateurs du domaine',
		)
	})

	it('labels a count with the agent and the option', () => {
		expect(
			exclusionOptionLabel({ agent_name: 'Exclusions', label: 'Concurrent' }),
		).toBe('Exclusions : Concurrent')
	})
})
