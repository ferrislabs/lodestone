import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useNavigate, useSearch } from '@tanstack/react-router'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import {
	queryFromSearch,
	searchForPage,
	searchForProfile,
	searchForVerdicts,
	verdictsFromSearch,
} from '#/pages/drafts/lib/search'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftsList } from '#/pages/drafts/ui/drafts-list'

const NO_ENTRIES: DraftEntry[] = []

export function DraftsListFeature() {
	const search = useSearch({ from: '/_app/drafts/' })
	const navigate = useNavigate()

	const profilesQuery = useQuery(
		lodestoneApi().get('/api/v1/profiles').queryOptions,
	)
	const draftsQuery = useQuery({
		...lodestoneApi().get('/api/v1/drafts', {
			query: queryFromSearch(search),
		}).queryOptions,
		placeholderData: keepPreviousData,
	})

	const failed = draftsQuery.isError
	const state = failed ? 'error' : draftsQuery.isPending ? 'loading' : 'ready'

	return (
		<DraftsList
			entries={draftsQuery.data?.data ?? NO_ENTRIES}
			state={state}
			errorMessage={failed ? errorMessage(draftsQuery.error) : null}
			profiles={profilesQuery.data?.data ?? []}
			profileId={search.profile ?? ''}
			onProfileChange={(profile) =>
				void navigate({
					to: '/drafts',
					search: searchForProfile(profile, search),
				})
			}
			verdicts={verdictsFromSearch(search)}
			onVerdictsChange={(verdicts) =>
				void navigate({
					to: '/drafts',
					search: searchForVerdicts(search, verdicts),
				})
			}
			pagination={draftsQuery.data?.pagination ?? null}
			onPageChange={(page) =>
				void navigate({ to: '/drafts', search: searchForPage(search, page) })
			}
			onOpen={(assessmentId) =>
				void navigate({
					to: '/drafts/$assessmentId',
					params: { assessmentId },
				})
			}
		/>
	)
}
