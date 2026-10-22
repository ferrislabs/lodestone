import { describe, expect, it } from 'vitest'

import type { DroppedCandidate } from '#/pages/quality/lib/types'

import {
	agentLabel,
	DROP_REASONS,
	droppedEntries,
	droppedQueryFor,
	droppedReviewHref,
	droppedSearchFor,
	droppedSearchSchema,
	dropReasonLabel,
	dropReviewLabel,
	NO_AGENT_LABEL,
	recallSearch,
	reviewStateOf,
	SAMPLE_SIZE,
} from './dropped'

describe('labels', () => {
	it('names each drop reason in French', () => {
		expect(DROP_REASONS.map(dropReasonLabel)).toEqual([
			'Aucun mot-clé',
			'Mot exclu',
			'Filtre télétravail',
			'Filtre pays',
			'Filtre contrat',
		])
	})

	it('falls back to the raw reason when it is unknown', () => {
		expect(dropReasonLabel('something_new')).toBe('something_new')
	})

	it('names each review', () => {
		expect(dropReviewLabel('rightly_dropped')).toBe('Bien rejetée')
		expect(dropReviewLabel('should_have_kept')).toBe('Aurait dû être gardée')
	})

	it('shows the slug of an agent, or the company job offers without one', () => {
		expect(agentLabel('offres-france')).toBe('offres-france')
		expect(agentLabel(null)).toBe(NO_AGENT_LABEL)
		expect(agentLabel(undefined)).toBe("Offres d'emploi de l'entreprise")
	})
})

describe('search', () => {
	it('keeps the profile as text and ignores an unknown reason', () => {
		expect(droppedSearchSchema.parse({ profile: 'p1' })).toEqual({
			profile: 'p1',
		})
		expect(droppedSearchSchema.parse({ profile: 12 }).profile).toBe('12')
		expect(droppedSearchSchema.parse({ reason: 'nope' }).reason).toBeUndefined()
		expect(droppedSearchSchema.parse({ reason: 'exclusion' }).reason).toBe(
			'exclusion',
		)
	})

	it('reviews the unreviewed ones unless told otherwise', () => {
		expect(reviewStateOf(droppedSearchSchema.parse({}))).toBe('todo')
		expect(reviewStateOf(droppedSearchSchema.parse({ reviewed: 'nope' }))).toBe(
			'todo',
		)
		expect(reviewStateOf(droppedSearchSchema.parse({ reviewed: 'done' }))).toBe(
			'done',
		)
		expect(reviewStateOf(droppedSearchSchema.parse({ reviewed: 'all' }))).toBe(
			'all',
		)
	})

	it('keeps only what differs from the defaults', () => {
		expect(
			droppedSearchFor({
				profile: 'p1',
				reason: '',
				agent: null,
				state: 'todo',
			}),
		).toEqual({ profile: 'p1' })
		expect(
			droppedSearchFor({
				profile: 'p1',
				reason: 'filter_country',
				agent: 'offres',
				state: 'done',
			}),
		).toEqual({
			profile: 'p1',
			reason: 'filter_country',
			agent: 'offres',
			reviewed: 'done',
		})
		expect(
			droppedSearchFor({ profile: '', reason: '', agent: null, state: 'all' }),
		).toEqual({ reviewed: 'all' })
	})

	it('opens a recall row on its profile, reason and agent', () => {
		expect(
			recallSearch({ profile_id: 'p1', reason: 'no_keyword', agent: 'offres' }),
		).toEqual({ profile: 'p1', reason: 'no_keyword', agent: 'offres' })
		expect(
			recallSearch({ profile_id: 'p1', reason: 'exclusion', agent: null }),
		).toEqual({ profile: 'p1', reason: 'exclusion' })
	})

	it('links to the review page with the profile', () => {
		expect(droppedReviewHref('')).toBe('/quality/dropped')
		expect(droppedReviewHref('p 1')).toBe('/quality/dropped?profile=p%201')
	})
})

describe('droppedQueryFor', () => {
	it('draws a random sample of the unreviewed ones', () => {
		expect(droppedQueryFor({ profile: 'p1' })).toEqual({
			reviewed: false,
			order: 'random',
			limit: SAMPLE_SIZE,
		})
		expect(SAMPLE_SIZE).toBe(20)
	})

	it('maps the review state to the reviewed filter', () => {
		expect(droppedQueryFor({ reviewed: 'done' }).reviewed).toBe(true)
		expect(droppedQueryFor({ reviewed: 'all' })).not.toHaveProperty('reviewed')
	})

	it('passes the reason and the agent', () => {
		expect(droppedQueryFor({ reason: 'exclusion', agent: 'offres' })).toEqual({
			reason: 'exclusion',
			agent: 'offres',
			reviewed: false,
			order: 'random',
			limit: 20,
		})
	})
})

const CANDIDATE: DroppedCandidate = {
	id: 'd1',
	profile_id: 'p1',
	agent: 'offres',
	source: 'france_travail',
	kind: 'job_offer',
	domain: 'acme.test',
	title: 'Développeur',
	excerpts: [],
	provenance: 'https://jobs.acme.test/1',
	observed_at: '2026-10-01T08:00:00Z',
	reason: 'no_keyword',
	matched: [],
	excluded: [],
	dropped_at: '2026-10-02T09:30:00Z',
	review: null,
	reviewed_at: null,
}

describe('droppedEntries', () => {
	it('keeps the sample as it is when nothing happened', () => {
		expect(droppedEntries([CANDIDATE], {}, {}, [])).toEqual([
			{ candidate: CANDIDATE, leadId: null, error: null, pending: false },
		])
	})

	it('replaces a candidate by its reviewed version and keeps its lead', () => {
		const reviewed = { ...CANDIDATE, review: 'should_have_kept' as const }
		expect(
			droppedEntries(
				[CANDIDATE, { ...CANDIDATE, id: 'd2' }],
				{ d1: { candidate: reviewed, leadId: 'l1' } },
				{},
				[],
			).map((entry) => [entry.candidate.review, entry.leadId]),
		).toEqual([
			['should_have_kept', 'l1'],
			[null, null],
		])
	})

	it('flags the pending and failed candidates', () => {
		const [first, second] = droppedEntries(
			[CANDIDATE, { ...CANDIDATE, id: 'd2' }],
			{},
			{ d2: 'Déjà revue' },
			['d1'],
		)
		expect([first.pending, first.error]).toEqual([true, null])
		expect([second.pending, second.error]).toEqual([false, 'Déjà revue'])
	})
})
