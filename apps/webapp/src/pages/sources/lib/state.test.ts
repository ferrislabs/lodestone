import { describe, expect, it } from 'vitest'

import type { Source, SourceRun } from '#/pages/sources/lib/types'
import {
	activeTriggers,
	canRun,
	groupedSources,
	isSettled,
	runDisabledReason,
	runSummary,
	shortError,
	shouldPoll,
	TRIGGER_TTL_MS,
} from './state'

const NOW = Date.parse('2026-10-05T12:00:00Z')

function source(name: string, overrides: Partial<Source> = {}): Source {
	return {
		source: name,
		kind: 'source.run',
		label_key: `source.${name}`,
		status: 'pending',
		mode: 'scheduled',
		attempts: 0,
		next_run_at: '2026-10-05T18:00:00Z',
		last_runs: [],
		...overrides,
	}
}

function run(overrides: Partial<SourceRun> = {}): SourceRun {
	return {
		profile_id: 'p1',
		profile_name: 'Fintech',
		status: 'succeeded',
		volume: 12,
		signals: 1,
		started_at: '2026-10-05T11:55:00Z',
		finished_at: '2026-10-05T11:56:00Z',
		...overrides,
	}
}

describe('groupedSources', () => {
	it('groups sources like the agent families and drops empty groups', () => {
		const groups = groupedSources([
			source('llm_qualification', { kind: 'llm.qualify' }),
			source('boamp'),
			source('http_probe'),
			source('role_guess'),
		])
		expect(groups.map((group) => group.title)).toEqual([
			'Infrastructure',
			'Marchés publics',
			'Contacts',
			'Qualification',
		])
		expect(groups[0]?.sources.map((item) => item.source)).toEqual([
			'http_probe',
		])
	})

	it('keeps an unknown source in a trailing group', () => {
		const groups = groupedSources([source('mystery')])
		expect(groups).toEqual([
			{ title: 'Autres', continuous: false, sources: [source('mystery')] },
		])
	})
})

describe('continuous sources', () => {
	const listening = (name: string) =>
		source(name, {
			mode: 'continuous',
			status: 'listening',
			next_run_at: null,
		})

	it('collects them first in their own group and out of the families', () => {
		const groups = groupedSources([
			source('boamp'),
			listening('dns_cname'),
			listening('role_guess'),
			listening('certificate_transparency'),
		])
		expect(groups.map((group) => group.title)).toEqual([
			'Écoute continue',
			'Marchés publics',
		])
		expect(groups[0]?.continuous).toBe(true)
		expect(groups[0]?.sources.map((item) => item.source)).toEqual([
			'certificate_transparency',
			'dns_cname',
			'role_guess',
		])
	})

	it('keeps polling while a listener is shown', () => {
		expect(shouldPoll([listening('dns_cname')], {}, NOW)).toBe(true)
		expect(shouldPoll([source('boamp')], {}, NOW)).toBe(false)
	})
})

describe('run availability', () => {
	it('only a pending source can be run', () => {
		expect(canRun(source('boamp'))).toBe(true)
		for (const status of ['running', 'failed', 'succeeded', 'unavailable']) {
			expect(canRun(source('boamp', { status }))).toBe(false)
			expect(runDisabledReason(source('boamp', { status }))).not.toBeNull()
		}
		expect(runDisabledReason(source('boamp'))).toBeNull()
	})

	it('explains missing credentials', () => {
		expect(
			runDisabledReason(
				source('france_travail', {
					status: 'unavailable',
					unavailable_reason: 'missing_credentials',
				}),
			),
		).toBe('Identifiants manquants : la source est désactivée')
	})
})

describe('polling', () => {
	const triggeredAt = NOW - 1_000

	it('polls while a source is running', () => {
		expect(shouldPoll([source('boamp', { status: 'running' })], {}, NOW)).toBe(
			true,
		)
		expect(shouldPoll([source('boamp')], {}, NOW)).toBe(false)
	})

	it('polls while a triggered source is still due', () => {
		const due = source('boamp', { next_run_at: '2026-10-05T11:59:59Z' })
		expect(shouldPoll([due], { boamp: triggeredAt }, NOW)).toBe(true)
		expect([...activeTriggers([due], { boamp: triggeredAt }, NOW)]).toEqual([
			'boamp',
		])
	})

	it('settles once the job is rescheduled after the trigger', () => {
		const later = source('boamp', { next_run_at: '2026-10-05T18:00:00Z' })
		expect(isSettled(later, triggeredAt, NOW)).toBe(true)
		expect(shouldPoll([later], { boamp: triggeredAt }, NOW)).toBe(false)
	})

	it('settles on failure and after the time to live', () => {
		expect(
			isSettled(source('boamp', { status: 'failed' }), triggeredAt, NOW),
		).toBe(true)
		const due = source('boamp', { next_run_at: '2026-10-05T11:59:59Z' })
		expect(isSettled(due, NOW - TRIGGER_TTL_MS - 1, NOW)).toBe(true)
	})
})

describe('runSummary', () => {
	it('summarises a finished run', () => {
		const summary = runSummary(run(), NOW)
		expect(summary.statusLabel).toBe('Réussi')
		expect(summary.counts).toBe('12 éléments, 1 signal')
		expect(summary.error).toBeNull()
		expect(summary.when).toBe('il y a 4 minutes')
		expect(summary.profile).toBe('Fintech')
	})

	it('shortens the error of a failed run', () => {
		const summary = runSummary(
			run({ status: 'failed', error: `${'x'.repeat(200)}` }),
			NOW,
		)
		expect(summary.failed).toBe(true)
		expect(summary.statusLabel).toBe('Échec')
		expect(summary.error?.length).toBe(80)
		expect(summary.error?.endsWith('…')).toBe(true)
		expect(shortError('a\n b')).toBe('a b')
	})

	it('has no counts while running', () => {
		const summary = runSummary(
			run({ status: 'running', finished_at: null }),
			NOW,
		)
		expect(summary.counts).toBeNull()
		expect(summary.statusLabel).toBe('En cours')
	})
})
