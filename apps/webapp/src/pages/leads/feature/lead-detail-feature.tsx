import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { nextStatuses } from '#/pages/leads/lib/transitions'
import { LeadDetail } from '#/pages/leads/ui/lead-detail'

const COMPANY_POLL_MS = 4_000

export function LeadDetailFeature() {
	const { leadId } = useParams({ from: '/_app/leads/$leadId' })
	const navigate = useNavigate()
	const queryClient = useQueryClient()

	const detailQuery = useQuery({
		...lodestoneApi().get('/api/v1/leads/{lead_id}', {
			path: { lead_id: leadId },
		}).queryOptions,
		refetchInterval: (query) =>
			query.state.data?.data.company?.state === 'pending'
				? COMPANY_POLL_MS
				: false,
	})

	const changeStatus = useMutation({
		...lodestoneApi().mutation('patch', '/api/v1/leads/{lead_id}/status')
			.mutationOptions,
		onSuccess: async () => {
			await Promise.all([
				queryClient.invalidateQueries({
					queryKey: [{ _id: '/api/v1/leads/{lead_id}' }],
				}),
				queryClient.invalidateQueries({
					queryKey: [{ _id: '/api/v1/profiles/{profile_id}/leads' }],
				}),
				queryClient.invalidateQueries({
					queryKey: [{ _id: '/api/v1/profiles' }],
				}),
			])
		},
	})

	const retrySummary = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/leads/{lead_id}/summary/retry')
			.mutationOptions,
		onSuccess: () =>
			queryClient.invalidateQueries({
				queryKey: [{ _id: '/api/v1/leads/{lead_id}' }],
			}),
	})

	const refreshCompany = useMutation({
		...lodestoneApi().mutation(
			'post',
			'/api/v1/leads/{lead_id}/company/refresh',
		).mutationOptions,
		onSuccess: () =>
			queryClient.invalidateQueries({
				queryKey: [{ _id: '/api/v1/leads/{lead_id}' }],
			}),
	})

	const detail = detailQuery.data?.data ?? null

	return (
		<LeadDetail
			state={
				detailQuery.isError
					? 'error'
					: detailQuery.isPending
						? 'loading'
						: 'ready'
			}
			errorMessage={
				detailQuery.isError ? errorMessage(detailQuery.error) : null
			}
			detail={detail}
			nextStatuses={detail ? nextStatuses(detail.lead.status) : []}
			onBack={() => {
				void navigate({
					to: '/leads',
					search: detail ? { profile: detail.lead.profile.id } : {},
				})
			}}
			onChangeStatus={(status) =>
				changeStatus.mutate({
					path: { lead_id: leadId },
					body: { status },
				})
			}
			onOpenDraft={(assessmentId) => {
				void navigate({
					to: '/drafts/$assessmentId',
					params: { assessmentId },
				})
			}}
			onRetrySummary={() => retrySummary.mutate({ path: { lead_id: leadId } })}
			retryingSummary={retrySummary.isPending}
			retrySummaryError={
				retrySummary.isError ? errorMessage(retrySummary.error) : null
			}
			onRefreshCompany={() =>
				refreshCompany.mutate({ path: { lead_id: leadId } })
			}
			refreshingCompany={refreshCompany.isPending}
			refreshCompanyError={
				refreshCompany.isError ? errorMessage(refreshCompany.error) : null
			}
			changing={changeStatus.isPending}
			changeError={
				changeStatus.isError ? errorMessage(changeStatus.error) : null
			}
		/>
	)
}
