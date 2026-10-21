import { describe, expect, it } from 'vitest'

import {
	actorText,
	FAST_POLL_MS,
	filterRounds,
	formatDuration,
	type LeadTimeline,
	nextText,
	type PipelineStage,
	presentSteps,
	roundTitle,
	SLOW_POLL_MS,
	type TimelineEvent,
	type TimelineRound,
	timelinePollMs,
	toggleStep,
} from './timeline'

const NOW = Date.parse('2026-10-09T10:00:00Z')

function event(overrides: Partial<TimelineEvent>): TimelineEvent {
	return {
		id: 'e1',
		round: 1,
		step: 'qualification',
		kind: 'agent',
		status: 'succeeded',
		actor: {},
		detail: {},
		started_at: '2026-10-09T09:00:00Z',
		reconstructed: false,
		...overrides,
	}
}

function round(overrides: Partial<TimelineRound>): TimelineRound {
	return {
		number: 1,
		trigger: 'detection',
		sources: [],
		source_labels: [],
		reconstructed: false,
		events: [],
		...overrides,
	}
}

function stage(overrides: Partial<PipelineStage>): PipelineStage {
	return {
		stage: 'qualification',
		status: 'done',
		label: 'Qualification',
		...overrides,
	}
}

function timeline(
	stages: PipelineStage[],
	inFlight: LeadTimeline['in_flight'] = [],
): LeadTimeline {
	return {
		lead_id: 'l1',
		rounds: [],
		in_flight: inFlight,
		pipeline: { stages },
	}
}

describe('roundTitle', () => {
	it('names the trigger and the sources of the new information', () => {
		expect(
			roundTitle(
				round({
					number: 3,
					trigger: 'new_information',
					sources: ['http_probe', 'website_stack'],
					source_labels: ['http_probe', 'Site et stack technique'],
				}),
			),
		).toBe(
			'Évaluation n°3 — déclenchée par : nouvelles informations (Sonde HTTP, Site et stack technique)',
		)
	})

	it('names a manual reevaluation and a round still in preparation', () => {
		expect(roundTitle(round({ number: 2, trigger: 'manual' }))).toBe(
			'Évaluation n°2 — déclenchée par : réévaluation manuelle',
		)
		expect(roundTitle(round({ number: 4, trigger: 'pending' }))).toBe(
			'Évaluation n°4 — en préparation',
		)
	})
})

describe('timelinePollMs', () => {
	it('polls fast while a step runs', () => {
		expect(timelinePollMs(timeline([stage({ status: 'running' })]), NOW)).toBe(
			FAST_POLL_MS,
		)
		expect(
			timelinePollMs(
				timeline(
					[stage({})],
					[
						{
							step: 'inspection',
							actor: {},
							started_at: '2026-10-09T09:59:00Z',
						},
					],
				),
				NOW,
			),
		).toBe(FAST_POLL_MS)
	})

	it('polls fast when the next attempt is near and slowly otherwise', () => {
		expect(
			timelinePollMs(
				timeline([
					stage({ status: 'waiting', next_at: '2026-10-09T10:01:00Z' }),
				]),
				NOW,
			),
		).toBe(FAST_POLL_MS)
		expect(
			timelinePollMs(
				timeline([
					stage({ status: 'waiting', next_at: '2026-10-09T12:00:00Z' }),
				]),
				NOW,
			),
		).toBe(SLOW_POLL_MS)
		expect(timelinePollMs(null, NOW)).toBe(SLOW_POLL_MS)
	})
})

describe('filters', () => {
	const rounds = [
		round({
			number: 2,
			events: [
				event({ id: 'a', step: 'scoring' }),
				event({ id: 'b', step: 'detection' }),
			],
		}),
		round({ number: 1, events: [event({ id: 'c', step: 'detection' })] }),
	]

	it('lists the steps present in their pipeline order', () => {
		expect(presentSteps(rounds)).toEqual(['detection', 'scoring'])
	})

	it('keeps only the selected steps and drops empty rounds', () => {
		const scoring = filterRounds(rounds, ['scoring'])
		expect(scoring).toHaveLength(1)
		expect(scoring[0].events.map((kept) => kept.id)).toEqual(['a'])
		expect(filterRounds(rounds, [])).toHaveLength(2)
	})

	it('toggles a step in and out of the selection', () => {
		expect(toggleStep([], 'scoring')).toEqual(['scoring'])
		expect(toggleStep(['scoring', 'stop'], 'scoring')).toEqual(['stop'])
	})
})

describe('formatting', () => {
	it('formats durations', () => {
		expect(formatDuration(null)).toBeNull()
		expect(formatDuration(420)).toBe('420 ms')
		expect(formatDuration(2_400)).toBe('2,4 s')
		expect(formatDuration(125_000)).toBe('2 min 5 s')
	})

	it('names the actor with its model', () => {
		expect(
			actorText({ agent_name: 'Stack', model: 'qwen', provider: 'Ollama' }),
		).toBe('Stack · qwen (Ollama)')
		expect(actorText({ source: 'http_probe' })).toBe('Sonde HTTP')
		expect(
			actorText({ module: 'dns_email', module_label: 'Écosystème DNS' }),
		).toBe('Écosystème DNS')
		expect(actorText({})).toBeNull()
	})

	it('describes the next action and its time', () => {
		expect(nextText(stage({}), NOW)).toBeNull()
		expect(nextText(stage({ next_action: 'Première évaluation' }), NOW)).toBe(
			'Prochaine étape : Première évaluation',
		)
		expect(
			nextText(
				stage({
					next_action: 'Nouvel essai',
					next_at: '2026-10-09T09:00:00Z',
				}),
				NOW,
			),
		).toBe('Prochaine étape : Nouvel essai (imminent)')
		expect(
			nextText(
				stage({
					next_action: 'Requalification',
					next_at: '2026-10-09T11:00:00Z',
				}),
				NOW,
			),
		).toMatch(/^Prochaine étape : Requalification à \d{2}:\d{2}$/)
	})
})
