import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { recalibrationNotice } from '#/pages/quality/lib/calibration'
import {
	DEFAULT_REVIEW_STATE,
	droppedSearchFor,
	recallSearch,
} from '#/pages/quality/lib/dropped'
import { clipboardText } from '#/pages/quality/lib/exclusions'
import { periodOf, queryFor, searchFor } from '#/pages/quality/lib/period'
import type { ExclusionCandidate } from '#/pages/quality/lib/types'
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
