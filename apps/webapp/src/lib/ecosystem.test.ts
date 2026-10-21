import { describe, expect, it } from 'vitest'

import {
	findingRows,
	formatDuration,
	moduleNote,
	moduleStatusLabel,
	platformNotice,
	statusTone,
} from '#/lib/ecosystem'

describe('ecosystem module statuses', () => {
	it('labels every status in French with its tone', () => {
		expect(moduleStatusLabel('pending')).toBe('En attente')
		expect(moduleStatusLabel('done')).toBe('Fait')
		expect(moduleStatusLabel('not_applicable')).toBe('Non applicable')
		expect(moduleStatusLabel('failed')).toBe('Échec')
		expect(statusTone('done')).toBe('done')
		expect(statusTone('not_applicable')).toBe('skipped')
	})

	it('gives the reason of a module that does not apply', () => {
		expect(moduleNote('not_applicable', 'SIREN inconnu')).toBe('SIREN inconnu')
		expect(moduleNote('failed', 'BODACC answered 500')).toBe(
			'BODACC answered 500',
		)
		expect(moduleNote('done', 'ignoré')).toBeNull()
		expect(moduleNote('not_applicable', null)).toBeNull()
	})

	it('explains the hosting platform notice', () => {
		expect(platformNotice('github.io')).toBe(
			"Site hébergé sur une plateforme (github.io) : les modules qui lisent le site ne s'appliquent pas.",
		)
		expect(platformNotice(null)).toBeNull()
		expect(platformNotice(undefined)).toBeNull()
	})
})

const BODACC_API =
	'https://bodacc-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/annonces-commerciales/records'

describe('finding rows', () => {
	it('keeps the source of every finding and flags highlighted offers', () => {
		const rows = findingRows([
			{
				module: 'hiring_activity',
				key: 'job_offer:https://jobs.acme.fr/1',
				kind: 'job_offer',
				label: "Offre d'emploi",
				value: 'Ingénieur IAM',
				source_url: 'https://jobs.acme.fr/1',
				observed_at: '2026-10-08T09:00:00Z',
				details: { highlighted: true, keywords: ['IAM'] },
			},
			{
				module: 'hiring_activity',
				key: 'open_positions:greenhouse (acme)',
				kind: 'open_positions',
				label: 'Offres publiées',
				value: '2',
				source_url: 'https://boards-api.greenhouse.io/v1/boards/acme/jobs',
				observed_at: '2026-10-08T09:00:00Z',
				details: {},
			},
		])
		expect(rows).toEqual([
			{
				key: 'job_offer:https://jobs.acme.fr/1',
				label: "Offre d'emploi",
				value: 'Ingénieur IAM',
				sourceUrl: 'https://jobs.acme.fr/1',
				highlighted: true,
				date: null,
				amount: null,
				badge: null,
				facts: [],
				link: null,
			},
			{
				key: 'open_positions:greenhouse (acme)',
				label: 'Offres publiées',
				value: '2',
				sourceUrl: 'https://boards-api.greenhouse.io/v1/boards/acme/jobs',
				highlighted: false,
				date: null,
				amount: null,
				badge: null,
				facts: [],
				link: null,
			},
		])
	})

	it('dates a legal announcement and flags a collective proceeding', () => {
		const [collective, accounts] = findingRows([
			{
				module: 'legal_life',
				key: 'bodacc:A202601733059',
				kind: 'collective_proceeding',
				label: 'Procédure collective',
				value: "Jugement d'ouverture de liquidation judiciaire",
				source_url: BODACC_API,
				observed_at: '2026-09-10T00:00:00Z',
				details: {
					date: '2026-09-10',
					court: 'Greffe du Tribunal de commerce de Lyon',
					collective: true,
					link: 'https://www.bodacc.fr/pages/annonces-commerciales-detail/?q.id=id:A202601733059',
				},
			},
			{
				module: 'legal_life',
				key: 'bodacc:C1',
				kind: 'accounts_filing',
				label: 'Dépôt des comptes',
				value: 'Comptes annuels et rapports, clôture le 2025-12-31',
				source_url: BODACC_API,
				observed_at: '2026-06-02T00:00:00Z',
				details: { date: '2026-06-02', collective: false, link: null },
			},
		])
		expect(collective.date).toBe('10 sept. 2026')
		expect(collective.badge).toBe('Procédure collective')
		expect(collective.facts).toEqual(['Greffe du Tribunal de commerce de Lyon'])
		expect(collective.link).toBe(
			'https://www.bodacc.fr/pages/annonces-commerciales-detail/?q.id=id:A202601733059',
		)
		expect(collective.sourceUrl).toBe(BODACC_API)
		expect(accounts.badge).toBeNull()
		expect(accounts.link).toBeNull()
		expect(accounts.date).toBe('2 juin 2026')
	})

	it('shows the buyer, the amount in euros and the procedure of a contract won', () => {
		const [award, unnamed] = findingRows([
			{
				module: 'public_contracts',
				key: 'contract:1',
				kind: 'contract',
				label: 'Marché remporté',
				value: 'Diagnostic des réseaux',
				source_url: 'https://tabular-api.data.gouv.fr/api/resources/r/data/',
				observed_at: '2026-08-10T00:00:00Z',
				details: {
					date: '2026-08-10',
					buyer: 'Commune de Lorient',
					buyer_siret: '21560121200016',
					amount_eur: 400000,
					procedure: 'Procédure adaptée',
				},
			},
			{
				module: 'public_contracts',
				key: 'contract:2',
				kind: 'contract',
				label: 'Marché remporté',
				value: 'Maintenance',
				source_url: 'https://tabular-api.data.gouv.fr/api/resources/r/data/',
				observed_at: '2026-08-10T00:00:00Z',
				details: {
					date: '2026-08-10',
					buyer: null,
					buyer_siret: '13000190200068',
				},
			},
		])
		expect(award.date).toBe('10 août 2026')
		expect(award.amount?.replace(/\s/g, ' ')).toBe('400 000 €')
		expect(award.facts).toEqual(['Commune de Lorient', 'Procédure adaptée'])
		expect(unnamed.amount).toBeNull()
		expect(unnamed.facts).toEqual(['Acheteur SIRET 13000190200068'])
	})

	it('writes the creation date of the company in french', () => {
		const [created] = findingRows([
			{
				module: 'official_identity',
				key: 'created_on',
				kind: 'created_on',
				label: 'Date de création',
				value: '2004-06-09',
				source_url:
					'https://recherche-entreprises.api.gouv.fr/search?q=552100554',
				observed_at: '2026-10-08T09:00:00Z',
				details: JSON.parse('null'),
			},
		])
		expect(created.value).toBe('9 juin 2004')
		expect(created.date).toBeNull()
	})

	it('formats a duration in milliseconds or seconds', () => {
		expect(formatDuration(350)).toBe('350 ms')
		expect(formatDuration(1250)).toBe('1,3 s')
	})
})
