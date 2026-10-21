import { useQuery } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { clipboardText } from '#/pages/quality/lib/exclusions'
import { periodOf, queryFor, searchFor } from '#/pages/quality/lib/period'
import type { ExclusionCandidate } from '#/pages/quality/lib/types'
import { QualityPage } from '#/pages/quality/ui/quality-page'

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
