import { describe, expect, it } from 'vitest'
import {
	enrichmentStatusLabel,
	listWaitingLabel,
	moduleSections,
	signalTitle,
	waitingBadge,
} from '#/pages/leads/lib/enrichment'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { Enrichment, Signal } from '#/pages/leads/lib/types'

const WAIT = '2026-10-08T10:30:00Z'

function enrichment(overrides: Partial<Enrichment> = {}): Enrichment {
	return {
		status: 'pending',
		modules: [],
		waiting_reason: 'awaiting_enrichment',
		wait_until: WAIT,
		...overrides,
	}
}

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
			{
				module: 'legal_life',
				label: 'Vie légale',
				statusLabel: 'Non applicable',
				tone: 'skipped',
				lastRun: null,
				note: 'SIREN inconnu',
				failed: false,
				providerLink: '/sources/providers/bodacc',
				rows: [],
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
				rows: [],
			},
		])
	})
})

describe('waitingBadge', () => {
	it('shows the wait for enrichment with its deadline', () => {
		expect(waitingBadge(enrichment())).toEqual({
			reason: 'awaiting_enrichment',
			label: "En attente d'enrichissement",
			detail: `jusqu'au ${formatDateTime(WAIT)}`,
		})
		expect(waitingBadge(enrichment({ wait_until: null }))?.detail).toBeNull()
	})

	it('shows the wait for evidence of a certificate-only lead', () => {
		expect(
			waitingBadge(
				enrichment({
					status: 'complete',
					waiting_reason: 'awaiting_enrichment_evidence',
					wait_until: null,
				}),
			),
		).toEqual({
			reason: 'awaiting_enrichment_evidence',
			label: "En attente de preuve d'enrichissement",
			detail: null,
		})
	})

	it('shows nothing when the lead is not waiting', () => {
		expect(
			waitingBadge(enrichment({ status: 'complete', waiting_reason: null })),
		).toBeNull()
		expect(waitingBadge(undefined)).toBeNull()
	})
})

describe('listWaitingLabel', () => {
	it('names the wait of a listed lead', () => {
		expect(
			listWaitingLabel({
				waiting_reason: 'awaiting_enrichment',
				wait_until: WAIT,
			}),
		).toBe("En attente d'enrichissement")
		expect(
			listWaitingLabel({ waiting_reason: 'awaiting_enrichment_evidence' }),
		).toBe("En attente de preuve d'enrichissement")
		expect(listWaitingLabel({ waiting_reason: null })).toBeNull()
		expect(listWaitingLabel(null)).toBeNull()
		expect(listWaitingLabel(undefined)).toBeNull()
	})
})

describe('enrichmentStatusLabel', () => {
	it('translates every status', () => {
		expect(enrichmentStatusLabel('disabled')).toBe('Inspection désactivée')
		expect(enrichmentStatusLabel('pending')).toBe('En cours')
		expect(enrichmentStatusLabel('complete')).toBe('Terminé')
		expect(enrichmentStatusLabel('timed_out')).toBe('Délai dépassé')
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
