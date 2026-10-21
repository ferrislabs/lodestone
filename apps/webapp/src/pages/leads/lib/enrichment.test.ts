import { describe, expect, it } from 'vitest'
import {
	moduleSections,
	signalExcerpts,
	signalTitle,
} from '#/pages/leads/lib/enrichment'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { Signal } from '#/pages/leads/lib/types'

const WAIT = '2026-10-08T10:30:00Z'

describe('moduleSections', () => {
	it('groups the findings per module with their status and sources', () => {
		const sections = moduleSections({
			hosting_platform: null,
			modules: [
				{
					module: 'official_identity',
					provider_ready: true,
					label: 'Identité officielle',
					status: 'done',
					last_run: '2026-10-08T10:05:00Z',
					findings: [
						{
							module: 'official_identity',
							key: 'legal_name',
							kind: 'legal_name',
							label: 'Raison sociale',
							value: 'ACME SAS',
							source_url:
								'https://recherche-entreprises.api.gouv.fr/search?q=552100554',
							observed_at: '2026-10-08T10:05:00Z',
							details: {},
						},
					],
				},
				{
					module: 'legal_life',
					provider: 'bodacc',
					provider_ready: false,
					label: 'Vie légale',
					status: 'not_applicable',
					error: 'SIREN inconnu',
					findings: [],
				},
				{
					module: 'hiring_activity',
					provider_ready: true,
					label: 'Activité de recrutement',
					status: 'failed',
					error: 'home page not confirmed',
					findings: [],
				},
			],
		})
		expect(sections).toEqual([
			{
				module: 'official_identity',
				label: 'Identité officielle',
				statusLabel: 'Fait',
				tone: 'done',
				lastRun: formatDateTime('2026-10-08T10:05:00Z'),
				note: null,
				failed: false,
				providerLink: null,
				view: {
					kind: 'list',
					rows: [
						{
							key: 'legal_name',
							label: 'Raison sociale',
							value: 'ACME SAS',
							sourceUrl:
								'https://recherche-entreprises.api.gouv.fr/search?q=552100554',
							highlighted: false,
							date: null,
							amount: null,
							badge: null,
							facts: [],
							link: null,
						},
					],
				},
			},
			{
				module: 'legal_life',
				label: 'Vie légale',
				statusLabel: 'Non applicable',
				tone: 'skipped',
				lastRun: null,
				note: 'SIREN inconnu',
				failed: false,
				providerLink: '/sources/providers/bodacc',
				view: { kind: 'list', rows: [] },
			},
			{
				module: 'hiring_activity',
				label: 'Activité de recrutement',
				statusLabel: 'Échec',
				tone: 'failed',
				lastRun: null,
				note: 'home page not confirmed',
				failed: true,
				providerLink: null,
				view: { kind: 'hiring', summary: null, rows: [] },
			},
		])
	})
})

describe('signalTitle', () => {
	const base: Signal = {
		id: 's1',
		kind: 'certificate_seen',
		value: 'sso.acme.fr',
		source: 'certificate_transparency',
		sentence: 'sso.acme.fr apparaît dans les certificats publics',
		strength: 0.3,
		observed_at: WAIT,
		provenance: 'https://crt.sh/?q=sso.acme.fr',
		raw: {},
	}

	it('names the host of a certificate signal', () => {
		expect(signalTitle(base)).toBe('Certificat observé pour sso.acme.fr')
	})

	it('keeps the sentence of other signals', () => {
		expect(signalTitle({ ...base, kind: 'job_offer' })).toBe(base.sentence)
		expect(signalTitle({ ...base, value: null })).toBe(base.sentence)
	})
})

describe('signalExcerpts', () => {
	const base: Signal = {
		id: 's1',
		kind: 'job_offer',
		value: 'Ingénieur IAM',
		source: 'france_travail',
		sentence: "Offre d'emploi : Ingénieur IAM",
		strength: 0.7,
		observed_at: WAIT,
		provenance: 'https://candidat.francetravail.fr/offres/1',
		raw: {},
	}

	it('reads the passages stored with the signal, in order', () => {
		const raw = {
			excerpts: ['…migration vers Keycloak pour le SSO…', 'annuaire LDAP'],
		}
		expect(signalExcerpts({ ...base, raw })).toEqual([
			'…migration vers Keycloak pour le SSO…',
			'annuaire LDAP',
		])
	})

	it('keeps non-empty strings only', () => {
		const raw = {
			excerpts: ['un', 3, null, '', '   ', { a: 1 }, ['x'], 'deux'],
		}
		expect(signalExcerpts({ ...base, raw })).toEqual(['un', 'deux'])
	})

	it('shows a repeated passage once', () => {
		const raw = { excerpts: ['SSO obligatoire', ' SSO obligatoire ', 'MFA'] }
		expect(signalExcerpts({ ...base, raw })).toEqual(['SSO obligatoire', 'MFA'])
	})

	it('is empty for a signal stored without passages', () => {
		expect(signalExcerpts(base)).toEqual([])
		expect(signalExcerpts({ ...base, raw: { excerpts: [] } })).toEqual([])
		expect(signalExcerpts({ ...base, raw: { excerpts: 'texte' } })).toEqual([])
		expect(signalExcerpts({ ...base, raw: { excerpts: null } })).toEqual([])
	})

	it('tolerates any raw shape', () => {
		for (const raw of [undefined, null, 'excerpts', 4, ['excerpts']]) {
			const signal = { ...base, raw } as unknown as Signal
			expect(signalExcerpts(signal)).toEqual([])
		}
	})
})
