import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import {
	type DroppedOutcome,
	type DroppedSearch,
	droppedEntries,
	droppedQueryFor,
	droppedSearchFor,
	type ReviewState,
	reviewStateOf,
} from '#/pages/quality/lib/dropped'
import type { DroppedCandidate, DropReason } from '#/pages/quality/lib/types'
import { DroppedPage } from '#/pages/quality/ui/dropped-page'

const NO_CANDIDATES: DroppedCandidate[] = []

export function DroppedFeature() {
	const search = useSearch({ from: '/_app/quality/dropped' })
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const profile = search.profile ?? ''
	const reason = search.reason ?? ''
	const agent = search.agent ?? null
	const reviewState = reviewStateOf(search)
	const [outcomes, setOutcomes] = useState<Record<string, DroppedOutcome>>({})
	const [errors, setErrors] = useState<Record<string, string>>({})
	const [pending, setPending] = useState<string[]>([])

	const profilesQuery = useQuery(
		lodestoneApi().get('/api/v1/profiles').queryOptions,
	)
	const droppedQuery = useQuery({
		...lodestoneApi().get('/api/v1/profiles/{profile_id}/dropped', {
			path: { profile_id: profile },
			query: droppedQueryFor(search),
		}).queryOptions,
		enabled: profile !== '',
		gcTime: 0,
		refetchOnWindowFocus: false,
		refetchOnReconnect: false,
	})
	const review = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/dropped/{dropped_id}/review')
			.mutationOptions,
		onMutate: ({ path }) => {
			setPending((ids) => [...ids, path.dropped_id])
			setErrors(({ [path.dropped_id]: _, ...rest }) => rest)
		},
		onSuccess: ({ data }) => {
			setOutcomes((current) => ({
				...current,
				[data.candidate.id]: {
					candidate: data.candidate,
					leadId: data.lead_id ?? null,
				},
			}))
			void queryClient.invalidateQueries({
				queryKey: [{ _id: '/api/v1/quality' }],
			})
		},
		onError: (error, { path }) =>
			setErrors((current) => ({
				...current,
				[path.dropped_id]: errorMessage(error),
			})),
		onSettled: (_data, _error, { path }) =>
			setPending((ids) => ids.filter((id) => id !== path.dropped_id)),
	})

	const state = droppedQuery.isError
		? 'error'
		: droppedQuery.isPending
			? 'loading'
			: 'ready'

	const go = (next: DroppedSearch) =>
		void navigate({ to: '/quality/dropped', search: next })
	const filters = { profile, reason, agent, state: reviewState } as const

	return (
		<DroppedPage
			profiles={profilesQuery.data?.data ?? []}
			profileId={profile}
			onProfileChange={(next) =>
				go(droppedSearchFor({ ...filters, profile: next, agent: null }))
			}
			reason={reason}
			onReasonChange={(next: DropReason | '') =>
				go(droppedSearchFor({ ...filters, reason: next }))
			}
			agent={agent}
			onClearAgent={() => go(droppedSearchFor({ ...filters, agent: null }))}
			reviewState={reviewState}
			onReviewStateChange={(next: ReviewState) =>
				go(droppedSearchFor({ ...filters, state: next }))
			}
			state={state}
			errorMessage={
				droppedQuery.isError ? errorMessage(droppedQuery.error) : null
			}
			entries={droppedEntries(
				droppedQuery.data?.data ?? NO_CANDIDATES,
				outcomes,
				errors,
				pending,
			)}
			refreshing={droppedQuery.isFetching}
			onRefresh={() => {
				setErrors({})
				void droppedQuery.refetch()
			}}
			onReview={(candidate, decision) =>
				review.mutate({
					path: { dropped_id: candidate.id },
					body: { review: decision },
				})
			}
			onOpenLead={(leadId) =>
				void navigate({ to: '/leads/$leadId', params: { leadId } })
			}
		/>
	)
}
