import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { invalidateProfiles } from '#/pages/profiles/feature/invalidate'
import { recalibrationNotice } from '#/pages/quality/lib/calibration'
import {
	DEFAULT_REVIEW_STATE,
	droppedSearchFor,
	recallSearch,
} from '#/pages/quality/lib/dropped'
import { clipboardText } from '#/pages/quality/lib/exclusions'
import { periodOf, queryFor, searchFor } from '#/pages/quality/lib/period'
import {
	listedSuggestions,
	type SuggestionOutcome,
	suggestionKey,
	type TrackedSuggestion,
} from '#/pages/quality/lib/sourcing'
import type {
	ExclusionCandidate,
	KeywordSuggestion,
} from '#/pages/quality/lib/types'
import { QualityPage } from '#/pages/quality/ui/quality-page'

const NO_PARAMETERS = {} as never

export function QualityFeature() {
	const search = useSearch({ from: '/_app/quality/' })
	const navigate = useNavigate()
	const period = periodOf(search)
	const profile = search.profile ?? ''
	const [copiedLeadId, setCopiedLeadId] = useState<string | null>(null)
	const [copyFailed, setCopyFailed] = useState(false)
	const copyExclusion = (candidate: ExclusionCandidate) => {
		setCopyFailed(false)
		navigator.clipboard
			.writeText(clipboardText(candidate))
			.then(() => setCopiedLeadId(candidate.lead_id))
			.catch(() => setCopyFailed(true))
	}
	const [expandedAgents, setExpandedAgents] = useState<readonly string[]>([])
	const [tracked, setTracked] = useState<
		Readonly<Record<string, TrackedSuggestion>>
	>({})
	const query = useMemo(
		() => queryFor({ period, profile: search.profile }, Date.now()),
		[period, search.profile],
	)

	const profilesQuery = useQuery(
		lodestoneApi().get('/api/v1/profiles').queryOptions,
	)
	const qualityQuery = useQuery(
		lodestoneApi().get('/api/v1/quality', { query }).queryOptions,
	)
	const calibrationQuery = useQuery(
		lodestoneApi().get('/api/v1/quality/calibration').queryOptions,
	)
	const queryClient = useQueryClient()
	const recalibrate = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/quality/calibration/run')
			.mutationOptions,
		onSuccess: () =>
			queryClient.invalidateQueries({
				queryKey: [{ _id: '/api/v1/quality/calibration' }],
			}),
	})
	const apply = useMutation(
		lodestoneApi().mutation(
			'post',
			'/api/v1/profiles/{profile_id}/keyword-suggestions/apply',
		).mutationOptions,
	)
	const track = (suggestion: KeywordSuggestion, outcome: SuggestionOutcome) =>
		setTracked((current) => ({
			...current,
			[suggestionKey(suggestion)]: { suggestion, outcome },
		}))
	const applySuggestion = async (suggestion: KeywordSuggestion) => {
		track(suggestion, { status: 'pending' })
		try {
			const current = await queryClient.fetchQuery({
				...lodestoneApi().get('/api/v1/profiles/{profile_id}', {
					path: { profile_id: suggestion.profile_id },
				}).queryOptions,
				staleTime: 0,
			})
			const applied = await apply.mutateAsync({
				path: { profile_id: suggestion.profile_id },
				body: {
					agent_id: suggestion.agent_id,
					action: suggestion.action,
					word: suggestion.word,
					revision: current.data.revision,
				},
			})
			track(suggestion, {
				status: 'applied',
				revision: applied.data.revision,
			})
			void queryClient.invalidateQueries({
				queryKey: [{ _id: '/api/v1/quality' }],
			})
			void invalidateProfiles(queryClient)
		} catch (error) {
			track(suggestion, { status: 'failed', message: errorMessage(error) })
		}
	}
	const toggleAgent = (key: string) =>
		setExpandedAgents((current) =>
			current.includes(key)
				? current.filter((candidate) => candidate !== key)
				: [...current, key],
		)
	const state = qualityQuery.isError
		? 'error'
		: qualityQuery.isPending
			? 'loading'
			: 'ready'

	return (
		<QualityPage
			quality={qualityQuery.data?.data ?? null}
			state={state}
			errorMessage={
				qualityQuery.isError ? errorMessage(qualityQuery.error) : null
			}
			profiles={profilesQuery.data?.data ?? []}
			profileId={profile}
			onProfileChange={(next) =>
				void navigate({ to: '/quality', search: searchFor(period, next) })
			}
			period={period}
			onPeriodChange={(next) =>
				void navigate({ to: '/quality', search: searchFor(next, profile) })
			}
			copiedLeadId={copiedLeadId}
			copyFailed={copyFailed}
			onCopyExclusion={copyExclusion}
			calibration={calibrationQuery.data?.data ?? null}
			calibrationRunning={recalibrate.isPending}
			calibrationNotice={
				recalibrate.isSuccess
					? recalibrationNotice(recalibrate.data.data.queued)
					: null
			}
			calibrationError={
				recalibrate.isError
					? errorMessage(recalibrate.error)
					: calibrationQuery.isError
						? errorMessage(calibrationQuery.error)
						: null
			}
			onRecalibrate={() => recalibrate.mutate(NO_PARAMETERS)}
			onReviewTriage={(row) =>
				void navigate({ to: '/quality/dropped', search: recallSearch(row) })
			}
			onOpenDropped={(profileId) =>
				void navigate({
					to: '/quality/dropped',
					search: droppedSearchFor({
						profile: profileId,
						reason: '',
						agent: null,
						state: DEFAULT_REVIEW_STATE,
					}),
				})
			}
			expandedAgents={expandedAgents}
			onToggleAgent={toggleAgent}
			suggestions={listedSuggestions(
				qualityQuery.data?.data.keyword_suggestions ?? [],
				tracked,
				profile,
			)}
			suggestionOutcomes={Object.fromEntries(
				Object.entries(tracked).map(([key, entry]) => [key, entry.outcome]),
			)}
			onApplySuggestion={(suggestion) => void applySuggestion(suggestion)}
			onOpenProfile={(profileId) =>
				void navigate({
					to: '/profiles/$profileId',
					params: { profileId },
					search: { tab: 'qualification' },
				})
			}
		/>
	)
}
