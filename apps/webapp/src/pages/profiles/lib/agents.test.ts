import { describe, expect, it } from 'vitest'

import {
	AGENT_KINDS,
	agentKind,
	agentProviders,
	agentSummary,
	agentTypeLabel,
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
		const first = newAgent('search', [])
		const second = newAgent('search', [first])
		const third = newAgent('search', [first, second])
		expect([first.id, second.id, third.id]).toEqual([
			'recherche',
			'recherche-2',
			'recherche-3',
		])
	})

	it('builds a default agent for every kind', () => {
		expect(newAgent('search', [], ['boamp', 'ted'])).toMatchObject({
			type: 'search',
			enabled: true,
			providers: ['boamp', 'ted'],
			keywords: [],
			exclusions: [],
			filters: { remote: 'any', countries: [], contracts: [] },
			accept_min_keywords: 2,
		})
		for (const kind of AGENT_KINDS.filter((kind) => kind !== 'search')) {
			expect(newAgent(kind, [])).toMatchObject({ type: kind, enabled: true })
		}
		expect(newAgent('certificates', [])).toMatchObject({ labels: [] })
		expect(newAgent('dns', [])).toMatchObject({ patterns: [] })
	})

	it('derives the id from the name until it is edited', () => {
		const agents = [newAgent('search', [])]
		const renamed = renameAgent(agents, 0, 'Public Tenders é')
		expect(renamed[0]).toMatchObject({ id: 'public-tenders-e' })
		const edited = [{ ...newAgent('search', []), id: 'mine' }]
		expect(renameAgent(edited, 0, 'Other')[0]).toMatchObject({ id: 'mine' })
	})

	it('slugifies', () => {
		expect(slugify('  DNS CNAME!! ')).toBe('dns-cname')
	})

	it('validates count, slug and uniqueness', () => {
		expect(validateAgents({ agents: [] })).toEqual([
			{ path: 'agents', message: 'Doit contenir au moins un agent' },
		])
		expect(
			validateAgents({
				agents: [],
				enrichment: { http_probe: { enabled: true } },
			}),
		).toEqual([])
		const a = { ...newAgent('search', [], ['ted']), id: 'Bad Id' }
		const b = newAgent('search', [], ['ted'])
		const c = newAgent('search', [], ['ted'])
		const paths = validateAgents({ agents: [a, b, c] }).map(
			(problem) => problem.path,
		)
		expect(paths).toEqual(['agents[0].id', 'agents[2].id'])
	})

	it('requires a provider on a search agent', () => {
		const empty = newAgent('search', [])
		expect(validateAgents({ agents: [empty] })).toEqual([
			{
				path: 'agents[0].providers',
				message: 'Choisir au moins un fournisseur',
			},
		])
	})

	it('flags a missing dependency', () => {
		const dns = newAgent('dns', [])
		expect(dependencyNote(dns, [dns])?.warn).toBe(true)
		const ct = newAgent('certificates', [])
		expect(dependencyNote(dns, [dns, ct])?.warn).toBe(false)
		const boamp = newAgent('search', [], ['boamp'])
		expect(dependencyNote(boamp, [dns])).toBeNull()
		expect(dependencyNote(ct, [ct])).toBeNull()
	})

	it('refuses to save without agents', () => {
		const attempt = prepareSave({
			schema_version: 14,
			identity: { offer: 'o', target_customer: 't' },
			agents: [],
			contact_ranking: { preferred_local_parts: [] },
			language: 'fr',
		})
		expect(attempt.ok).toBe(false)
	})

	it('counts problems per tab', () => {
		expect(
			problemCounts([
				{ path: 'agents[1].source.keywords[0]', message: 'x' },
				{ path: 'contact_ranking.preferred_local_parts[0]', message: 'x' },
				{ path: 'language', message: 'x' },
				{ path: '$', message: 'x' },
			]),
		).toEqual({
			identity: 1,
			sourcing: 2,
			qualification: 0,
			scoring: 0,
		})
	})
})

describe('agentSummary', () => {
	it('summarizes search agents', () => {
		expect(
			agentSummary({
				type: 'search',
				providers: ['boamp'],
				keywords: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'],
				exclusions: ['x', 'y', 'z'],
			}),
		).toBe('8 mots-clés · 3 exclusions')
		expect(
			agentSummary({
				type: 'search',
				providers: ['ted'],
				keywords: ['a'],
				exclusions: [],
			}),
		).toBe('1 mot-clé')
	})

	it('summarizes certificates and dns agents', () => {
		expect(agentSummary({ type: 'certificates', labels: ['a', 'b'] })).toBe(
			'2 labels',
		)
		expect(agentSummary({ type: 'dns', patterns: [{}, {}, {}] })).toBe(
			'3 motifs CNAME',
		)
		expect(agentSummary({ type: 'dns', patterns: [{}] })).toBe('1 motif CNAME')
	})

	it('is empty for an unknown agent', () => {
		expect(agentSummary({ type: 'signal', source: { provider: 'nope' } })).toBe(
			'',
		)
		expect(agentSummary(undefined)).toBe('')
	})
})

describe('agent types', () => {
	it('names the type and the providers of an agent', () => {
		const search = newAgent('search', [], ['boamp', 'decp'])
		expect(agentKind(search)).toBe('search')
		expect(agentTypeLabel(search)).toBe('Recherche')
		expect(agentProviders(search)).toEqual(['boamp', 'decp'])
		const certificates = newAgent('certificates', [])
		expect(agentKind(certificates)).toBe('certificates')
		expect(agentTypeLabel(certificates)).toBe('Signal')
		expect(agentProviders(certificates)).toEqual(['certificate_transparency'])
		expect(agentProviders(newAgent('dns', []))).toEqual(['dns_cname'])
		expect(agentKind({ type: 'search' })).toBe('search')
		expect(agentKind({ source: { kind: 'ats' } })).toBeNull()
	})
})
