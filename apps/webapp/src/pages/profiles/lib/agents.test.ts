import { describe, expect, it } from 'vitest'

import {
	AGENT_KINDS,
	agentSummary,
	dependencyNote,
	newAgent,
	renameAgent,
	slugify,
	validateAgents,
} from '#/pages/profiles/lib/agents'
import { prepareSave } from '#/pages/profiles/lib/save'
import { problemCounts } from '#/pages/profiles/lib/tabs'

describe('agents', () => {
	it('suffixes ids that are taken', () => {
		const first = newAgent('ats', [])
		const second = newAgent('ats', [first])
		const third = newAgent('ats', [first, second])
		expect([first.id, second.id, third.id]).toEqual(['ats', 'ats-2', 'ats-3'])
	})

	it('builds a default agent for every kind', () => {
		for (const kind of AGENT_KINDS) {
			expect(newAgent(kind, [])).toMatchObject({
				enabled: true,
				source: { kind },
			})
		}
	})

	it('derives the id from the name until it is edited', () => {
		const agents = [newAgent('boamp', [])]
		const renamed = renameAgent(agents, 0, 'Public Tenders é')
		expect(renamed[0]).toMatchObject({ id: 'public-tenders-e' })
		const edited = [{ ...newAgent('boamp', []), id: 'mine' }]
		expect(renameAgent(edited, 0, 'Other')[0]).toMatchObject({ id: 'mine' })
	})

	it('slugifies', () => {
		expect(slugify('  DNS CNAME!! ')).toBe('dns-cname')
	})

	it('validates count, slug and uniqueness', () => {
		expect(validateAgents({ agents: [] })).toEqual([
			{ path: 'agents', message: 'Doit contenir au moins un agent' },
		])
		const a = { ...newAgent('ats', []), id: 'Bad Id' }
		const b = newAgent('ats', [])
		const c = newAgent('ats', [])
		const paths = validateAgents({ agents: [a, b, c] }).map(
			(problem) => problem.path,
		)
		expect(paths).toEqual(['agents[0].id', 'agents[2].id'])
	})

	it('flags a missing dependency', () => {
		const probe = newAgent('http_probe', [])
		expect(dependencyNote('http_probe', [probe])?.warn).toBe(true)
		const ct = newAgent('certificate_transparency', [])
		expect(dependencyNote('http_probe', [probe, ct])?.warn).toBe(false)
		expect(dependencyNote('boamp', [probe])).toBeNull()
	})

	it('refuses to save without agents', () => {
		const attempt = prepareSave({
			schema_version: 5,
			identity: { offer: 'o', target_customer: 't' },
			agents: [],
			scoring: { factors: [] },
			contact_ranking: { preferred_local_parts: [] },
			llm: { qualification_threshold: 50, relevance_criteria: 'c' },
		})
		expect(attempt.ok).toBe(false)
	})

	it('counts problems per tab', () => {
		expect(
			problemCounts([
				{ path: 'agents[1].source.keywords[0]', message: 'x' },
				{ path: 'llm.relevance_criteria', message: 'x' },
				{ path: '$', message: 'x' },
			]),
		).toEqual({
			identity: 0,
			sourcing: 1,
			qualification: 0,
			scoring: 0,
			settings: 1,
		})
	})
})

describe('agentSummary', () => {
	it('summarizes keyword sources', () => {
		expect(
			agentSummary({
				source: {
					kind: 'ats',
					keywords: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'],
					exclusions: ['x', 'y', 'z'],
				},
			}),
		).toBe('8 mots-clés · 3 exclusions')
		expect(
			agentSummary({
				source: { kind: 'ted', keywords: ['a'], exclusions: [] },
			}),
		).toBe('1 mot-clé')
	})

	it('summarizes infrastructure sources', () => {
		expect(
			agentSummary({
				source: { kind: 'certificate_transparency', labels: ['a', 'b'] },
			}),
		).toBe('2 labels')
		expect(
			agentSummary({ source: { kind: 'dns_cname', patterns: [{}, {}, {}] } }),
		).toBe('3 motifs CNAME')
		expect(
			agentSummary({
				source: {
					kind: 'http_probe',
					fingerprints: [{}, {}, {}, {}, {}],
					excluded_products: ['mine'],
				},
			}),
		).toBe('5 empreintes · 1 produit exclu')
		expect(
			agentSummary({ source: { kind: 'http_probe', fingerprints: [{}] } }),
		).toBe('1 empreinte')
	})

	it('is empty for an unknown source', () => {
		expect(agentSummary({ source: { kind: 'nope' } })).toBe('')
		expect(agentSummary(undefined)).toBe('')
	})
})
