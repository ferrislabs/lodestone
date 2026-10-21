import { describe, expect, it } from 'vitest'

import type { EcosystemFinding } from '#/lib/ecosystem'
import {
	dnsLines,
	hiringView,
	moduleView,
	recentView,
	techGroups,
} from '#/lib/ecosystem-views'

function finding(
	module: EcosystemFinding['module'],
	kind: string,
	label: string,
	value: string,
	details: Record<string, unknown> = {},
	source = 'https://acme.fr/',
): EcosystemFinding {
	return {
		module,
		key: `${kind}:${value}`,
		kind,
		label,
		value,
		source_url: source,
		observed_at: '2026-10-08T09:00:00Z',
		details,
	}
}

function tech(category: string, label: string, name: string, version?: string) {
	return finding(
		'website_stack',
		'technology',
		label,
		version ? `${name} ${version}` : name,
		{ category, name, version: version ?? null, evidence: 'en-tête server' },
	)
}

describe('technology groups', () => {
	it('groups the detected technologies by category in a fixed order', () => {
		const groups = techGroups([
			tech('analytics', "Mesure d'audience", 'Matomo'),
			tech('cms', 'CMS', 'WordPress', '6.4'),
			tech('hosting_cdn', 'Hébergement et CDN', 'Cloudflare'),
			tech('analytics', "Mesure d'audience", 'Hotjar'),
			finding('website_stack', 'server', 'Serveur web', 'nginx'),
		])
		expect(
			groups.map((group) => [
				group.label,
				group.chips.map((chip) =>
					chip.version ? `${chip.name} ${chip.version}` : chip.name,
				),
			]),
		).toEqual([
			['CMS', ['WordPress 6.4']],
			['Hébergement et CDN', ['Cloudflare']],
			["Mesure d'audience", ['Matomo', 'Hotjar']],
		])
		expect(groups[0].chips[0].evidence).toBe('en-tête server')
	})

	it('keeps the other website findings as rows', () => {
		const view = moduleView('website_stack', [
			tech('cms', 'CMS', 'WordPress'),
			finding('website_stack', 'product', 'Produit détecté', 'keycloak 21.1'),
		])
		expect(view.kind).toBe('website')
		expect(view.rows.map((row) => row.value)).toEqual(['keycloak 21.1'])
	})
})

describe('hiring view', () => {
	it('summarises counts, functions and the trend', () => {
		const view = hiringView([
			finding(
				'hiring_activity',
				'hiring_summary',
				'Recrutement',
				'12 offres ouvertes : 7 engineering, 3 sales, 2 other (+4 depuis le dernier passage)',
				{
					count: 12,
					delta: 4,
					remote: 3,
					functions: [
						{ function: 'engineering', count: 7 },
						{ function: 'sales', count: 3 },
						{ function: 'other', count: 2 },
					],
					locations: [{ name: 'Lyon', count: 8 }],
					contracts: [{ label: 'CDI', count: 10 }],
				},
			),
		])
		expect(view).toEqual({
			headline: '12 offres ouvertes',
			trend: { direction: 'up', label: '+4 depuis le dernier passage' },
			functions: [
				{ label: 'Tech et ingénierie', count: 7 },
				{ label: 'Commercial', count: 3 },
				{ label: 'Autres', count: 2 },
			],
			locations: [{ label: 'Lyon', count: 8 }],
			contracts: [{ label: 'CDI', count: 10 }],
			remote: '3 offres mentionnent le télétravail',
		})
	})

	it('shows a falling or stable trend and no trend on a first pass', () => {
		const summary = (details: Record<string, unknown>) =>
			hiringView([
				finding('hiring_activity', 'hiring_summary', 'Recrutement', 'x', {
					count: 1,
					...details,
				}),
			])
		expect(summary({ delta: -2 })?.trend).toEqual({
			direction: 'down',
			label: '−2 depuis le dernier passage',
		})
		expect(summary({ delta: 0 })?.trend?.direction).toBe('flat')
		expect(summary({ delta: null })?.trend).toBeNull()
		expect(summary({})?.headline).toBe('1 offre ouverte')
		expect(hiringView([])).toBeNull()
	})

	it('lists the offers with their place, contract and remote mention', () => {
		const view = moduleView('hiring_activity', [
			finding('hiring_activity', 'hiring_summary', 'Recrutement', 'x', {
				count: 1,
			}),
			finding(
				'hiring_activity',
				'job_offer',
				"Offre d'emploi",
				'Développeur Rust',
				{
					location: 'Nantes',
					contracts: ['CDI'],
					remote: true,
					posted_at: '2026-09-20',
				},
			),
		])
		expect(view.rows).toHaveLength(1)
		expect(view.rows[0].facts).toEqual(['Nantes', 'CDI', 'Télétravail'])
		expect(view.rows[0].date).toBe('20 sept. 2026')
	})
})

describe('recent view', () => {
	it('lists the latest items newest first with the freshness', () => {
		const view = recentView([
			finding(
				'recent_activity',
				'freshness',
				'Fraîcheur du site',
				'mis à jour il y a 6 jours (2026-10-02)',
				{ days: 6, latest: '2026-10-02' },
			),
			finding(
				'recent_activity',
				'latest_news',
				'Dernière actualité',
				'Sortie de la version 5 (2026-10-02)',
			),
			finding(
				'recent_activity',
				'recent_item',
				'Publication',
				'Salon des maires',
				{ date: '2026-09-15' },
				'https://acme.fr/actualites/salon',
			),
			finding(
				'recent_activity',
				'recent_item',
				'Publication',
				'Sortie de la version 5',
				{ date: '2026-10-02' },
				'https://acme.fr/actualites/v5',
			),
		])
		expect(view?.freshness).toBe('Mis à jour il y a 6 jours (2 oct. 2026)')
		expect(view?.latest).toBe('Sortie de la version 5 (2026-10-02)')
		expect(
			view?.items.map((item) => [item.title, item.date, item.url]),
		).toEqual([
			[
				'Sortie de la version 5',
				'2 oct. 2026',
				'https://acme.fr/actualites/v5',
			],
			['Salon des maires', '15 sept. 2026', 'https://acme.fr/actualites/salon'],
		])
		expect(recentView([])).toBeNull()
	})
})

describe('dns lines', () => {
	it('summarises mail, sending services, policies, tools and subdomains', () => {
		const lines = dnsLines([
			finding('dns_email', 'verified_tool', 'Outil vérifié (TXT)', 'Atlassian'),
			finding('dns_email', 'mail_provider', 'Messagerie', 'Microsoft 365 (MX)'),
			finding('dns_email', 'spf_service', "Envoi d'e-mails (SPF)", 'Mailjet'),
			finding('dns_email', 'spf', 'SPF', '-all (strict)'),
			finding('dns_email', 'dmarc', 'DMARC', 'p=reject (rejet)'),
			finding('dns_email', 'verified_tool', 'Outil vérifié (TXT)', 'Zoom'),
			finding(
				'dns_email',
				'saas_host',
				'Service sur sous-domaine',
				'Atlassian Statuspage (status.acme.fr)',
			),
		])
		expect(lines.map((line) => [line.title, line.values])).toEqual([
			['Messagerie', ['Microsoft 365 (MX)']],
			["Envoi d'e-mails (SPF)", ['Mailjet']],
			['SPF', ['-all (strict)']],
			['DMARC', ['p=reject (rejet)']],
			['Outils vérifiés (TXT)', ['Atlassian', 'Zoom']],
			['Services sur sous-domaines', ['Atlassian Statuspage (status.acme.fr)']],
		])
		expect(moduleView('dns_email', []).kind).toBe('dns')
		expect(moduleView('legal_life', []).kind).toBe('list')
	})
})
