import { describe, expect, it } from 'vitest'
import {
	enrichmentRows,
	enrichmentStatusLabel,
	listWaitingLabel,
	signalTitle,
	waitingBadge,
} from '#/pages/leads/lib/enrichment'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { Enrichment, Signal } from '#/pages/leads/lib/types'

const WAIT = '2026-10-08T10:30:00Z'

function enrichment(overrides: Partial<Enrichment> = {}): Enrichment {
	return {
		status: 'pending',
		steps: [
			{ step: 'http_probe', enabled: true, ran_at: '2026-10-08T10:05:00Z' },
			{ step: 'company_jobs', enabled: true, ran_at: null },
		],
		waiting_reason: 'awaiting_enrichment',
		wait_until: WAIT,
		...overrides,
	}
}

describe('enrichmentRows', () => {
	it('names each step with its state and date', () => {
		expect(
			enrichmentRows(
				enrichment({
					steps: [
						{
							step: 'http_probe',
							enabled: true,
							ran_at: '2026-10-08T10:05:00Z',
						},
						{ step: 'company_jobs', enabled: true, ran_at: null },
					],
				}),
			),
		).toEqual([
			{
				step: 'http_probe',
				label: 'Sonde HTTP',
				state: 'done',
				stateLabel: 'Fait',
				date: formatDateTime('2026-10-08T10:05:00Z'),
			},
			{
				step: 'company_jobs',
				label: "Offres d'emploi de l'entreprise",
				state: 'pending',
				stateLabel: 'En attente',
				date: null,
			},
		])
	})

	it('lists a disabled step as disabled even if it ran before', () => {
		const rows = enrichmentRows(
			enrichment({
				steps: [
					{ step: 'http_probe', enabled: false, ran_at: WAIT },
					{ step: 'company_jobs', enabled: false },
				],
			}),
		)
		expect(rows.map((row) => [row.state, row.stateLabel])).toEqual([
			['disabled', 'Désactivé'],
			['disabled', 'Désactivé'],
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
		expect(enrichmentStatusLabel('disabled')).toBe('Aucune étape activée')
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
