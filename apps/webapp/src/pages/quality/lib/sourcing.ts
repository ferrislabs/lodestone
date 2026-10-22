import { formatNumber } from '#/lib/format'
import type {
	AgentYield,
	KeywordSuggestion,
	KeywordYield,
	ProfileSummary,
} from '#/pages/quality/lib/types'

export type SuggestionOutcome =
	| { status: 'pending' }
	| { status: 'applied'; revision: number }
	| { status: 'failed'; message: string }

export interface TrackedSuggestion {
	suggestion: KeywordSuggestion
	outcome: SuggestionOutcome
}

export function agentKey(
	agent: Pick<AgentYield, 'profile_id' | 'agent_id'>,
): string {
	return `${agent.profile_id}:${agent.agent_id}`
}

export function suggestionKey(suggestion: KeywordSuggestion): string {
	return `${suggestion.profile_id}:${suggestion.agent_id}:${suggestion.action}:${suggestion.word}`
}

export function yieldAgentLabel(
	agent: Pick<AgentYield, 'agent_id' | 'agent_name'>,
): string {
	return agent.agent_name ?? agent.agent_id
}

function suggestionAgentLabel(
	suggestion: KeywordSuggestion,
	agents: readonly AgentYield[],
): string {
	const agent = agents.find(
		(candidate) =>
			candidate.profile_id === suggestion.profile_id &&
			candidate.agent_id === suggestion.agent_id,
	)
	return agent === undefined ? suggestion.agent_id : yieldAgentLabel(agent)
}

export function suggestionSentence(
	suggestion: KeywordSuggestion,
	agents: readonly AgentYield[],
): string {
	const agent = suggestionAgentLabel(suggestion, agents)
	if (suggestion.action === 'remove_keyword') {
		return `Retirer le mot-clé « ${suggestion.word} » de ${agent} : ${formatNumber(suggestion.decided)} leads décidés, aucun validé.`
	}
	return `Ajouter « ${suggestion.word} » aux exclusions de ${agent} : présent dans ${formatNumber(suggestion.rejected)} leads rejetés, aucun validé.`
}

export function profileName(
	profiles: readonly ProfileSummary[],
	profileId: string,
): string | null {
	return profiles.find((profile) => profile.id === profileId)?.name ?? null
}

export function spansSeveralProfiles(
	items: readonly { profile_id: string }[],
): boolean {
	return new Set(items.map((item) => item.profile_id)).size > 1
}

export function exclusiveShortLabel(
	keyword: Pick<KeywordYield, 'exclusive_leads'>,
): string | null {
	return keyword.exclusive_leads > 0
		? `dont ${formatNumber(keyword.exclusive_leads)} seul`
		: null
}

export function exclusiveLongLabel(
	keyword: Pick<KeywordYield, 'exclusive_leads'>,
): string | null {
	return keyword.exclusive_leads > 0
		? `dont ${formatNumber(keyword.exclusive_leads)} par ce seul mot-clé`
		: null
}

export function listedSuggestions(
	reported: readonly KeywordSuggestion[],
	tracked: Readonly<Record<string, TrackedSuggestion>>,
	profileId: string,
): KeywordSuggestion[] {
	const reportedKeys = new Set(reported.map(suggestionKey))
	const applied = Object.entries(tracked)
		.filter(
			([key, entry]) =>
				entry.outcome.status === 'applied' &&
				!reportedKeys.has(key) &&
				(profileId === '' || entry.suggestion.profile_id === profileId),
		)
		.map(([, entry]) => entry.suggestion)
	return [...reported, ...applied]
}
