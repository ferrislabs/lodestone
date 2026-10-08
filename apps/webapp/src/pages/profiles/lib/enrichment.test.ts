import { describe, expect, it } from 'vitest'

import type { JsonObject } from '#/lib/schema-form'
import { newAgent } from '#/pages/profiles/lib/agents'
import {
	advancedHasProblem,
	customFieldSchema,
	customSection,
	type DryRunResult,
	dryRunView,
	type EcosystemModuleInfo,
	ENRICHMENT_WAIT_MINUTES,
	enrichmentFieldSchema,
	inspectionEnabled,
	moduleCards,
	searchKeywordUnion,
	withInspectionEnabled,
} from '#/pages/profiles/lib/enrichment'

function search(keywords: string[], enabled = true): JsonObject {
	return { ...newAgent('search', [], ['boamp']), keywords, enabled }
}

function info(overrides: Partial<EcosystemModuleInfo>): EcosystemModuleInfo {
	return {
		module: 'hiring_activity',
		label: 'Activité de recrutement',
		question: 'Recrute-t-elle, et sur quels postes ?',
		learns: 'Page carrières, outil de recrutement et offres publiées.',
		available: true,
		reads_site: true,
		refresh_days: 3,
		stats: { leads_inspected: 0, findings_count: 0 },
		...overrides,
	}
}

describe('inspection switch', () => {
	it('is on unless the profile turns it off', () => {
		expect(inspectionEnabled({})).toBe(true)
		expect(inspectionEnabled({ inspection: { enabled: false } })).toBe(false)
		const off = withInspectionEnabled({ inspection: {} }, false)
		expect(off.inspection).toEqual({ enabled: false })
		expect(inspectionEnabled(withInspectionEnabled(off, true))).toBe(true)
	})
})

describe('profile-specific settings', () => {
	it('reads a missing custom section as empty defaults without trade subdomains', () => {
		expect(customSection({})).toEqual({
			excluded_products: [],
			extra_subdomains: [],
			fingerprints: [],
		})
	})

	it('picks one field of the custom section and the keywords field', () => {
		expect(
			Object.keys(customFieldSchema('fingerprints').properties ?? {}),
		).toEqual(['fingerprints'])
		expect(
			Object.keys(enrichmentFieldSchema('highlight_keywords').properties ?? {}),
		).toEqual(['highlight_keywords'])
	})

	it('opens Avancé when a custom field has a problem', () => {
		expect(
			advancedHasProblem([
				{ path: 'enrichment.custom.fingerprints[0].path', message: 'x' },
			]),
		).toBe(true)
		expect(
			advancedHasProblem([
				{ path: 'enrichment.highlight_keywords[0]', message: 'x' },
			]),
		).toBe(false)
	})

	it('waits thirty minutes by default', () => {
		expect(ENRICHMENT_WAIT_MINUTES).toBe(30)
	})
})

describe('module cards', () => {
	it('shows the question, the status and seven days of activity', () => {
		const [card] = moduleCards(
			[
				info({
					stats: {
						leads_inspected: 12,
						findings_count: 31,
						last_run: '2026-10-08T09:00:00Z',
						latest_example: {
							domain: 'acme.fr',
							label: "Offre d'emploi",
							value: 'Ingénieur IAM',
							source_url: 'https://jobs.acme.fr/1',
						},
					},
				}),
			],
			true,
		)
		expect(card.title).toBe('Recrute-t-elle, et sur quels postes ?')
		expect(card.chip).toEqual({ label: 'Actif', tone: 'done' })
		expect(card.stats.map((stat) => [stat.label, stat.value])).toEqual([
			['Leads inspectés', '12'],
			['Éléments trouvés', '31'],
			['Dernier passage', expect.stringContaining('2026')],
		])
		expect(card.example).toEqual({
			text: "Offre d'emploi : Ingénieur IAM",
			domain: 'acme.fr',
			sourceUrl: 'https://jobs.acme.fr/1',
		})
		expect(card.refresh).toBe('Rafraîchi tous les 3 jours')
		expect(card.canTest).toBe(true)
	})

	it('marks modules to come and the switched off inspection', () => {
		const cards = moduleCards(
			[
				info({ module: 'legal_life', available: false }),
				info({ module: 'website_stack' }),
			],
			false,
		)
		expect(cards[0].chip).toEqual({ label: 'À venir', tone: 'soon' })
		expect(cards[0].canTest).toBe(false)
		expect(cards[1].chip).toEqual({ label: 'Désactivé', tone: 'off' })
		expect(cards[1].stats[2].value).toBe('Jamais')
		expect(cards[1].example).toBeNull()
	})
})

describe('dry run view', () => {
	function result(overrides: Partial<DryRunResult>): DryRunResult {
		return {
			module: 'website_stack',
			domain: 'acme.fr',
			status: 'done',
			duration_ms: 1250,
			findings: [],
			...overrides,
		}
	}

	it('lists the findings with their sources and the time it took', () => {
		const view = dryRunView(
			result({
				findings: [
					{
						module: 'website_stack',
						key: 'server',
						kind: 'server',
						label: 'Serveur web',
						value: 'nginx',
						source_url: 'https://acme.fr/',
						observed_at: '2026-10-08T09:00:00Z',
						details: {},
					},
				],
			}),
		)
		expect(view.summary).toBe('acme.fr · Fait en 1,3 s · 1 élément trouvé')
		expect(view.rows[0]).toMatchObject({ label: 'Serveur web', value: 'nginx' })
		expect(view.notice).toBeNull()
	})

	it('explains a platform site and a failure', () => {
		expect(
			dryRunView(
				result({ status: 'not_applicable', hosting_platform: 'github.io' }),
			).notice,
		).toContain('github.io')
		const failed = dryRunView(
			result({ status: 'failed', error: 'home page not confirmed' }),
		)
		expect(failed.summary).toBe('acme.fr · Échec en 1,3 s · 0 élément trouvé')
		expect(failed.notice).toBe('home page not confirmed')
	})
})

describe('searchKeywordUnion', () => {
	it('merges the keywords of the enabled search agents once', () => {
		const value: JsonObject = {
			agents: [
				search(['Keycloak', 'SSO']),
				search(['sso', 'IAM']),
				search(['ignored'], false),
				newAgent('certificates', []),
			],
		}
		expect(searchKeywordUnion(value)).toEqual(['Keycloak', 'SSO', 'IAM'])
	})
})
