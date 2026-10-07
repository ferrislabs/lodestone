import { describe, expect, it } from 'vitest'

import {
	EDITOR_TABS,
	editorTabItems,
	problemCounts,
	TAB_LABELS,
	tabOfPath,
	tabSearchSchema,
} from '#/pages/profiles/lib/tabs'

describe('tabs', () => {
	it('orders sourcing, qualification and scoring after identity', () => {
		expect(EDITOR_TABS).toEqual([
			'identity',
			'sourcing',
			'qualification',
			'scoring',
			'settings',
		])
		expect(TAB_LABELS.sourcing).toBe('Sourcing')
		expect(TAB_LABELS.qualification).toBe('Qualification')
	})

	it('keeps the legacy agents tab as an alias of sourcing', () => {
		expect(tabSearchSchema.parse({ tab: 'agents' }).tab).toBe('sourcing')
		expect(tabSearchSchema.parse({ tab: 'qualification' }).tab).toBe(
			'qualification',
		)
		expect(tabSearchSchema.parse({ tab: 'history' }).tab).toBe('history')
		expect(tabSearchSchema.parse({ tab: 'nope' }).tab).toBeUndefined()
		expect(tabSearchSchema.parse({}).tab).toBeUndefined()
	})

	it('maps server paths to their tab', () => {
		expect(tabOfPath('agents[0].id')).toBe('sourcing')
		expect(tabOfPath('qualification_agents')).toBe('qualification')
		expect(tabOfPath('scoring_agents')).toBe('scoring')
		expect(tabOfPath('scoring_agents[0].criteria[1].id')).toBe('scoring')
		expect(tabOfPath('scoring.factors[0].weight')).toBeNull()
		expect(tabOfPath('evidence.weights.volume')).toBe('qualification')
		expect(tabOfPath('verdict.min_evidence')).toBe('qualification')
		expect(
			tabOfPath('qualification_agents[1].questions[0].options[2].score'),
		).toBe('qualification')
		expect(tabOfPath('aggregator.system_prompt')).toBe('qualification')
		expect(tabOfPath('verdict.reliable_from')).toBe('qualification')
		expect(tabOfPath('calibration.learned')).toBe('settings')
		expect(tabOfPath('llm.language')).toBe('settings')
		expect(tabOfPath('$')).toBeNull()
	})

	it('badges the qualification tab', () => {
		const counts = problemCounts([
			{ path: 'qualification_agents[0].name', message: 'x' },
			{ path: 'verdict.discard_below', message: 'x' },
		])
		expect(counts.qualification).toBe(2)
		const item = editorTabItems([
			{ path: 'aggregator.system_prompt', message: 'x' },
		]).find((tab) => tab.id === 'qualification')
		expect(item).toEqual({
			id: 'qualification',
			label: 'Qualification',
			problems: 1,
		})
	})
})
