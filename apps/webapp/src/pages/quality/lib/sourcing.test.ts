import { describe, expect, it } from 'vitest'

import {
	agentKey,
	exclusiveLongLabel,
	exclusiveShortLabel,
	listedSuggestions,
	profileName,
	spansSeveralProfiles,
	suggestionKey,
	suggestionSentence,
	type TrackedSuggestion,
	yieldAgentLabel,
} from '#/pages/quality/lib/sourcing'
import type {
	AgentYield,
	KeywordSuggestion,
	KeywordYield,
} from '#/pages/quality/lib/types'

const VERDICTS = { reliable: 0, to_verify: 0, discarded: 0, none: 0 }

function agent(overrides: Partial<AgentYield> = {}): AgentYield {
	return {
		profile_id: 'p1',
		agent_id: 'offres-france',
		agent_name: 'Offres France',
		leads: 10,
		verdicts: VERDICTS,
		validated: 0,
		rejected: 0,
		precision: null,
		keywords: [],
		...overrides,
	}
}

function keyword(overrides: Partial<KeywordYield> = {}): KeywordYield {
	return {
		keyword: 'cloud',
		leads: 4,
		exclusive_leads: 0,
		verdicts: VERDICTS,
		validated: 0,
		rejected: 0,
		precision: null,
		...overrides,
	}
}

function suggestion(
	overrides: Partial<KeywordSuggestion> = {},
): KeywordSuggestion {
	return {
		profile_id: 'p1',
		agent_id: 'offres-france',
		action: 'remove_keyword',
		word: 'stage',
		decided: 6,
		rejected: 6,
		...overrides,
	}
}

describe('suggestionSentence', () => {
	it('words the removal of a keyword with the agent name', () => {
		expect(suggestionSentence(suggestion(), [agent()])).toBe(
			'Retirer le mot-clé « stage » de Offres France : 6 leads décidés, aucun validé.',
		)
	})

	it('words an exclusion with the number of rejected leads', () => {
		expect(
			suggestionSentence(
				suggestion({
					action: 'add_exclusion',
					word: 'alternance',
					rejected: 4,
				}),
				[agent()],
			),
		).toBe(
			'Ajouter « alternance » aux exclusions de Offres France : présent dans 4 leads rejetés, aucun validé.',
		)
	})

	it('falls back to the agent id when the agent is unknown or left the profile', () => {
		expect(suggestionSentence(suggestion(), [])).toContain('de offres-france :')
		expect(
			suggestionSentence(suggestion(), [agent({ agent_name: null })]),
		).toContain('de offres-france :')
	})

	it('looks the agent up in the same profile only', () => {
		const other = agent({ profile_id: 'p2', agent_name: 'Autre profil' })
		expect(suggestionSentence(suggestion(), [other])).toContain(
			'de offres-france :',
		)
		expect(
			suggestionSentence(suggestion({ profile_id: 'p2' }), [agent(), other]),
		).toContain('de Autre profil :')
	})
})

describe('keys and labels', () => {
	it('keys a suggestion by profile, agent, action and word', () => {
		expect(suggestionKey(suggestion())).toBe(
			'p1:offres-france:remove_keyword:stage',
		)
		expect(
			suggestionKey(suggestion({ action: 'add_exclusion', word: 'stage' })),
		).toBe('p1:offres-france:add_exclusion:stage')
	})

	it('keys an agent by profile and agent id', () => {
		expect(agentKey(agent())).toBe('p1:offres-france')
	})

	it('labels an agent with its name or its id', () => {
		expect(yieldAgentLabel(agent())).toBe('Offres France')
		expect(yieldAgentLabel(agent({ agent_name: null }))).toBe('offres-france')
	})

	it('finds the profile name or nothing', () => {
		const profiles = [
			{ id: 'p1', name: 'Fintech', lead_count: 3, active: true },
		]
		expect(profileName(profiles, 'p1')).toBe('Fintech')
		expect(profileName(profiles, 'p9')).toBeNull()
	})

	it('tells when the report covers several profiles', () => {
		expect(spansSeveralProfiles([])).toBe(false)
		expect(spansSeveralProfiles([agent(), agent({ agent_id: 'b' })])).toBe(
			false,
		)
		expect(spansSeveralProfiles([agent(), agent({ profile_id: 'p2' })])).toBe(
			true,
		)
	})
})

describe('exclusive leads', () => {
	it('shows the leads that only this keyword finds', () => {
		expect(exclusiveShortLabel(keyword({ exclusive_leads: 2 }))).toBe(
			'dont 2 seul',
		)
		expect(exclusiveLongLabel(keyword({ exclusive_leads: 2 }))).toBe(
			'dont 2 par ce seul mot-clé',
		)
	})

	it('shows nothing when no lead depends on the keyword alone', () => {
		expect(exclusiveShortLabel(keyword())).toBeNull()
		expect(exclusiveLongLabel(keyword())).toBeNull()
	})
})

describe('listedSuggestions', () => {
	const applied: TrackedSuggestion = {
		suggestion: suggestion(),
		outcome: { status: 'applied', revision: 4 },
	}

	it('keeps the reported suggestions untouched', () => {
		const reported = [suggestion({ word: 'alpha' })]
		expect(listedSuggestions(reported, {}, '')).toEqual(reported)
	})

	it('keeps an applied suggestion the report no longer lists', () => {
		const reported = [suggestion({ word: 'alpha' })]
		expect(
			listedSuggestions(
				reported,
				{ [suggestionKey(applied.suggestion)]: applied },
				'',
			),
		).toEqual([...reported, applied.suggestion])
	})

	it('does not list an applied suggestion twice', () => {
		const reported = [suggestion()]
		expect(
			listedSuggestions(
				reported,
				{ [suggestionKey(applied.suggestion)]: applied },
				'',
			),
		).toEqual(reported)
	})

	it('forgets suggestions that are not applied', () => {
		const pending: TrackedSuggestion = {
			suggestion: suggestion(),
			outcome: { status: 'pending' },
		}
		const failed: TrackedSuggestion = {
			suggestion: suggestion({ word: 'beta' }),
			outcome: { status: 'failed', message: 'Conflit' },
		}
		expect(
			listedSuggestions(
				[],
				{
					[suggestionKey(pending.suggestion)]: pending,
					[suggestionKey(failed.suggestion)]: failed,
				},
				'',
			),
		).toEqual([])
	})

	it('keeps only the applied suggestions of the chosen profile', () => {
		const tracked = { [suggestionKey(applied.suggestion)]: applied }
		expect(listedSuggestions([], tracked, 'p2')).toEqual([])
		expect(listedSuggestions([], tracked, 'p1')).toEqual([applied.suggestion])
	})
})
