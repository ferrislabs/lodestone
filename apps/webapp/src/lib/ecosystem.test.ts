import { describe, expect, it } from 'vitest'

import {
	findingRows,
	formatDuration,
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

	it('explains the hosting platform notice', () => {
		expect(platformNotice('github.io')).toBe(
			"Site hébergé sur une plateforme (github.io) : les modules qui lisent le site ne s'appliquent pas.",
		)
		expect(platformNotice(null)).toBeNull()
		expect(platformNotice(undefined)).toBeNull()
	})
})

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
			},
			{
				key: 'open_positions:greenhouse (acme)',
				label: 'Offres publiées',
				value: '2',
				sourceUrl: 'https://boards-api.greenhouse.io/v1/boards/acme/jobs',
				highlighted: false,
			},
		])
	})

	it('formats a duration in milliseconds or seconds', () => {
		expect(formatDuration(350)).toBe('350 ms')
		expect(formatDuration(1250)).toBe('1,3 s')
	})
})
