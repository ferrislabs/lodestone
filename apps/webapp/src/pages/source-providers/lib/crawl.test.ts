import { describe, expect, it } from 'vitest'

import {
	crawlChangeNote,
	crawlPolicyLabel,
	ignoresRobots,
	ROBOTS_WARNING,
	retriedNote,
	robotsIgnoredNote,
} from '#/pages/source-providers/lib/crawl'
import { crawlSettings } from '#/pages/source-providers/lib/fixtures'

describe('crawl rules', () => {
	it('tells whether robots.txt is ignored', () => {
		expect(ignoresRobots(crawlSettings())).toBe(false)
		expect(ignoresRobots(crawlSettings({ robots_policy: 'ignore' }))).toBe(true)
		expect(ignoresRobots(null)).toBe(false)
	})

	it('describes the current policy in French', () => {
		expect(crawlPolicyLabel(false)).toBe(
			'robots.txt est respecté pour la lecture des sites.',
		)
		expect(crawlPolicyLabel(true)).toBe(
			'robots.txt est ignoré pour la lecture des sites.',
		)
	})

	it('says the server default applies until someone saves the rule', () => {
		expect(crawlChangeNote(crawlSettings())).toBe(
			'Valeur par défaut du serveur (CRAWL_ROBOTS_POLICY = respect), jamais modifiée ici.',
		)
	})

	it('says who changed the rule and when', () => {
		expect(
			crawlChangeNote(
				crawlSettings({
					robots_policy: 'ignore',
					origin: 'organisation',
					updated_by: 'u-1',
					updated_by_name: 'Alice Martin',
					updated_at: '2026-10-09T08:30:00Z',
				}),
			),
		).toBe('Modifié par Alice Martin le 9 octobre 2026 à 10:30.')
		expect(
			crawlChangeNote(
				crawlSettings({
					origin: 'organisation',
					updated_at: '2026-10-09T08:30:00Z',
				}),
			),
		).toBe('Modifié par un utilisateur supprimé le 9 octobre 2026 à 10:30.')
	})

	it('counts the requests read despite robots.txt', () => {
		expect(robotsIgnoredNote(0)).toBeNull()
		expect(robotsIgnoredNote(1)).toBe(
			'1 requête lue malgré un refus de robots.txt depuis le démarrage du serveur.',
		)
		expect(robotsIgnoredNote(3)).toBe(
			'3 requêtes lues malgré un refus de robots.txt depuis le démarrage du serveur.',
		)
	})

	it('announces the dossiers scheduled for a new reading', () => {
		expect(retriedNote(0)).toBeNull()
		expect(retriedNote(1)).toBe('1 dossier bloqué par robots.txt sera relu.')
		expect(retriedNote(4)).toBe(
			'4 dossiers bloqués par robots.txt seront relus.',
		)
	})

	it('keeps the legal warning', () => {
		expect(ROBOTS_WARNING).toBe(
			"robots.txt peut porter l'opposition du propriétaire du site à la fouille de textes et de données (directive (UE) 2019/790, art. 4 ; CPI, art. L.122-5-3). Ignorer robots.txt engage la responsabilité de l'opérateur. Les limites de débit et l'identification du robot restent appliquées.",
		)
	})
})
