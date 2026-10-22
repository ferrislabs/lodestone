import { describe, expect, it } from 'vitest'

import {
	asEditorTab,
	EDITOR_TABS,
	editorTabItems,
	isEditorTab,
	problemCounts,
	TAB_LABELS,
	tabOfPath,
	tabSearchSchema,
} from '#/pages/profiles/lib/tabs'

describe('tabs', () => {
	it('orders sourcing, enrichment, qualification and scoring after identity', () => {
		expect(EDITOR_TABS).toEqual([
			'identity',
			'sourcing',
			'enrichment',
			'qualification',
			'scoring',
		])
		expect(Object.keys(TAB_LABELS)).not.toContain('settings')
		expect(TAB_LABELS.sourcing).toBe('Sourcing')
		expect(TAB_LABELS.qualification).toBe('Qualification')
		expect(TAB_LABELS.enrichment).toBe('Enrichissement')
	})

	it('reads the enrichment tab from the search', () => {
		expect(tabSearchSchema.parse({ tab: 'enrichment' }).tab).toBe('enrichment')
	})

	it('keeps the legacy agents tab as an alias of sourcing', () => {
		expect(tabSearchSchema.parse({ tab: 'agents' }).tab).toBe('sourcing')
		expect(tabSearchSchema.parse({ tab: 'settings' }).tab).toBe('identity')
		expect(tabSearchSchema.parse({ tab: 'qualification' }).tab).toBe(
			'qualification',
		)
		expect(tabSearchSchema.parse({ tab: 'history' }).tab).toBe('history')
		expect(tabSearchSchema.parse({ tab: 'nope' }).tab).toBeUndefined()
		expect(tabSearchSchema.parse({}).tab).toBeUndefined()
	})

	it('reads the trials tab and the selected trial from the search', () => {
		expect(TAB_LABELS.trials).toBe('Essais')
		expect(tabSearchSchema.parse({ tab: 'trials', trial: 't1' })).toEqual({
			tab: 'trials',
			trial: 't1',
		})
		expect(tabSearchSchema.parse({ tab: 'trials' }).trial).toBeUndefined()
		expect(tabSearchSchema.parse({ trial: 4 }).trial).toBeUndefined()
	})

	it('tells editor tabs from the history and the trials', () => {
		expect(isEditorTab('scoring')).toBe(true)
		expect(isEditorTab('history')).toBe(false)
		expect(isEditorTab('trials')).toBe(false)
		expect(asEditorTab('trials')).toBe('identity')
		expect(asEditorTab('history')).toBe('identity')
		expect(asEditorTab('scoring')).toBe('scoring')
		expect(asEditorTab(undefined)).toBe('identity')
	})

	it('reads the open sourcing agent from the search', () => {
		expect(
			tabSearchSchema.parse({ tab: 'sourcing', agent: 'offres-missions' }),
		).toEqual({ tab: 'sourcing', agent: 'offres-missions' })
		expect(tabSearchSchema.parse({ agent: 3 }).agent).toBeUndefined()
	})

	it('maps server paths to their tab', () => {
		expect(tabOfPath('agents[0].id')).toBe('sourcing')
		expect(tabOfPath('enrichment')).toBe('enrichment')
		expect(tabOfPath('enrichment.http_probe.fingerprints[0].path')).toBe(
			'enrichment',
		)
		expect(tabOfPath('enrichment.company_jobs.keywords')).toBe('enrichment')
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
		expect(tabOfPath('language')).toBe('identity')
		expect(tabOfPath('contact_ranking.preferred_local_parts[0]')).toBe(
			'sourcing',
		)
		expect(tabOfPath('contact_ranking.guess_local_parts[1]')).toBe('sourcing')
		expect(tabOfPath('qualification_agents[0].learned_calibration')).toBe(
			'qualification',
		)
		expect(tabOfPath('scoring_agents[1].learned_calibration')).toBe('scoring')
		expect(tabOfPath('calibration.learned')).toBeNull()
		expect(tabOfPath('llm.language')).toBeNull()
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
