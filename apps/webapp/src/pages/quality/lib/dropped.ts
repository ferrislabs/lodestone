import { z } from 'zod'

import type {
	DroppedCandidate,
	DropReason,
	DropReview,
	TriageRecall,
} from '#/pages/quality/lib/types'

export const DROP_REASONS = [
	'no_keyword',
	'exclusion',
	'filter_remote',
	'filter_country',
	'filter_contract',
] as const satisfies readonly DropReason[]

const DROP_REASON_LABELS: Readonly<Record<DropReason, string>> = {
	no_keyword: 'Aucun mot-clé',
	exclusion: 'Mot exclu',
	filter_remote: 'Filtre télétravail',
	filter_country: 'Filtre pays',
	filter_contract: 'Filtre contrat',
}

export const DROP_REVIEWS = [
	'rightly_dropped',
	'should_have_kept',
] as const satisfies readonly DropReview[]

const DROP_REVIEW_LABELS: Readonly<Record<DropReview, string>> = {
	rightly_dropped: 'Bien rejetée',
	should_have_kept: 'Aurait dû être gardée',
}

export const REVIEW_STATES = ['todo', 'done', 'all'] as const

export type ReviewState = (typeof REVIEW_STATES)[number]

export const DEFAULT_REVIEW_STATE: ReviewState = 'todo'

export const REVIEW_STATE_LABELS: Readonly<Record<ReviewState, string>> = {
	todo: 'À revoir',
	done: 'Revues',
	all: 'Toutes',
}

export const NO_AGENT_LABEL = "Offres d'emploi de l'entreprise"

export const SAMPLE_SIZE = 20

const text = z.union([z.string(), z.number()]).transform(String)

export const droppedSearchSchema = z.object({
	profile: text.optional(),
	reason: z.enum(DROP_REASONS).optional().catch(undefined),
	agent: text.optional(),
	reviewed: z.enum(REVIEW_STATES).optional().catch(undefined),
})

export type DroppedSearch = z.output<typeof droppedSearchSchema>

export interface DroppedEntry {
	candidate: DroppedCandidate
	leadId: string | null
	error: string | null
	pending: boolean
}

export interface DroppedOutcome {
	candidate: DroppedCandidate
	leadId: string | null
}

export function dropReasonLabel(reason: string): string {
	return DROP_REASON_LABELS[reason as DropReason] ?? reason
}

export function dropReviewLabel(review: DropReview): string {
	return DROP_REVIEW_LABELS[review]
}

export function agentLabel(agent: string | null | undefined): string {
	return agent ?? NO_AGENT_LABEL
}

export function reviewStateOf(search: DroppedSearch): ReviewState {
	return search.reviewed ?? DEFAULT_REVIEW_STATE
}

function reviewedFilter(state: ReviewState): boolean | undefined {
	if (state === 'todo') return false
	if (state === 'done') return true
	return undefined
}

export function droppedQueryFor(search: DroppedSearch): {
	reason?: DropReason
	agent?: string
	reviewed?: boolean
	order: string
	limit: number
} {
	const reviewed = reviewedFilter(reviewStateOf(search))
	return {
		...(search.reason === undefined ? {} : { reason: search.reason }),
		...(search.agent ? { agent: search.agent } : {}),
		...(reviewed === undefined ? {} : { reviewed }),
		order: 'random',
		limit: SAMPLE_SIZE,
	}
}

export function droppedSearchFor(filters: {
	profile: string
	reason: DropReason | ''
	agent: string | null
	state: ReviewState
}): DroppedSearch {
	return {
		...(filters.profile === '' ? {} : { profile: filters.profile }),
		...(filters.reason === '' ? {} : { reason: filters.reason }),
		...(filters.agent === null ? {} : { agent: filters.agent }),
		...(filters.state === DEFAULT_REVIEW_STATE
			? {}
			: { reviewed: filters.state }),
	}
}

export function recallSearch(
	row: Pick<TriageRecall, 'profile_id' | 'reason' | 'agent'>,
): DroppedSearch {
	return droppedSearchFor({
		profile: row.profile_id,
		reason: row.reason,
		agent: row.agent ?? null,
		state: DEFAULT_REVIEW_STATE,
	})
}

export function droppedReviewHref(profile: string): string {
	return profile === ''
		? '/quality/dropped'
		: `/quality/dropped?profile=${encodeURIComponent(profile)}`
}

export function droppedEntries(
	candidates: readonly DroppedCandidate[],
	outcomes: Readonly<Record<string, DroppedOutcome>>,
	errors: Readonly<Record<string, string>>,
	pending: readonly string[],
): DroppedEntry[] {
	return candidates.map((candidate) => {
		const outcome = outcomes[candidate.id]
		return {
			candidate: outcome?.candidate ?? candidate,
			leadId: outcome?.leadId ?? null,
			error: errors[candidate.id] ?? null,
			pending: pending.includes(candidate.id),
		}
	})
}
