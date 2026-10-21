import { describe, expect, it, vi } from 'vitest'

import { agentRunState, runAgent } from '#/pages/profiles/lib/agent-run'
import { summary } from '#/pages/source-providers/lib/fixtures'

const CATALOG = [
	summary({ key: 'boamp', name: 'BOAMP' }),
	summary({ key: 'ted', name: 'TED' }),
	summary({ key: 'decp', name: 'DECP' }),
]

const NONE = new Set<string>()

describe('runAgent', () => {
	it('triggers every provider of the agent once', () => {
		const trigger = vi.fn()
		runAgent(['boamp', 'ted', 'boamp'], trigger)
		expect(trigger.mock.calls).toEqual([['boamp'], ['ted']])
	})
})

describe('agentRunState', () => {
	it('is idle without results', () => {
		expect(agentRunState(['boamp'], NONE, {}, CATALOG)).toEqual({
			pending: false,
			message: null,
			error: false,
		})
	})

	it('waits for every provider before summing up', () => {
		expect(
			agentRunState(
				['boamp', 'ted'],
				new Set(['ted']),
				{ boamp: { kind: 'scheduled' } },
				CATALOG,
			),
		).toEqual({ pending: true, message: null, error: false })
	})

	it('keeps the single provider wording', () => {
		expect(
			agentRunState(['boamp'], NONE, { boamp: { kind: 'scheduled' } }, CATALOG)
				.message,
		).toBe('Planifié')
		expect(
			agentRunState(['boamp'], NONE, { boamp: { kind: 'running' } }, CATALOG)
				.message,
		).toBe('Déjà en cours')
		expect(
			agentRunState(
				['boamp'],
				NONE,
				{ boamp: { kind: 'error', message: 'Non prêt' } },
				CATALOG,
			),
		).toEqual({ pending: false, message: 'Non prêt', error: true })
	})

	it('sums several providers up in one message', () => {
		expect(
			agentRunState(
				['boamp', 'ted'],
				NONE,
				{ boamp: { kind: 'scheduled' }, ted: { kind: 'scheduled' } },
				CATALOG,
			).message,
		).toBe('Agent lancé : 2 sources planifiées')
		expect(
			agentRunState(
				['boamp', 'ted', 'decp'],
				NONE,
				{
					boamp: { kind: 'scheduled' },
					ted: { kind: 'running' },
					decp: { kind: 'error', message: 'Non prêt' },
				},
				CATALOG,
			),
		).toEqual({
			pending: false,
			message: '1 source planifiée · 1 déjà en cours · DECP : Non prêt',
			error: true,
		})
	})
})
