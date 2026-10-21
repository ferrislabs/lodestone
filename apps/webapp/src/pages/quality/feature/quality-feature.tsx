import { useQuery } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { periodOf, queryFor, searchFor } from '#/pages/quality/lib/period'
import { QualityPage } from '#/pages/quality/ui/quality-page'

export function QualityFeature() {
	const search = useSearch({ from: '/_app/quality/' })
	const navigate = useNavigate()
	const period = periodOf(search)
	const profile = search.profile ?? ''
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
