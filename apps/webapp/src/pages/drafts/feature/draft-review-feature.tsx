import { useForm, useStore } from '@tanstack/react-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { valuesFromEntry } from '#/pages/drafts/lib/edit'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftEditor } from '#/pages/drafts/ui/draft-editor'
import { DraftReview } from '#/pages/drafts/ui/draft-review'
import { RejectForm } from '#/pages/drafts/ui/reject-form'

export function DraftReviewFeature() {
	const { assessmentId } = useParams({ from: '/_app/drafts/$assessmentId' })
	const navigate = useNavigate()

	const draftQuery = useQuery(
		lodestoneApi().get('/api/v1/drafts/{assessment_id}', {
			path: { assessment_id: assessmentId },
		}).queryOptions,
	)
	const entry = draftQuery.data?.data

	if (entry) {
		return <DraftReviewForms key={entry.assessment_id} entry={entry} />
	}

	return (
		<DraftReview
			state={draftQuery.isError ? 'error' : 'loading'}
			errorMessage={draftQuery.isError ? errorMessage(draftQuery.error) : null}
			entry={null}
			editor={null}
			rejectForm={null}
			rejectOpen={false}
			dirty={false}
			deciding={false}
			decisionError={null}
			onBack={() => void navigate({ to: '/drafts', search: {} })}
			onOpenLead={() => {}}
			onApprove={() => {}}
			onStartReject={() => {}}
		/>
	)
}

interface DraftReviewFormsProps {
	entry: DraftEntry
}

function DraftReviewForms({ entry }: DraftReviewFormsProps) {
	const assessmentId = entry.assessment_id
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const [rejectOpen, setRejectOpen] = useState(false)

	const invalidate = async () => {
		await Promise.all([
			queryClient.invalidateQueries({ queryKey: [{ _id: '/api/v1/drafts' }] }),
			queryClient.invalidateQueries({
				queryKey: [{ _id: '/api/v1/drafts/{assessment_id}' }],
			}),
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
	}

	const decided = async () => {
		await invalidate()
		void navigate({ to: '/drafts', search: {} })
	}

	const edit = useMutation({
		...lodestoneApi().mutation('patch', '/api/v1/drafts/{assessment_id}')
			.mutationOptions,
		onSuccess: async (response) => {
			draftForm.reset(valuesFromEntry(response.data))
			await invalidate()
		},
	})
	const approve = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/drafts/{assessment_id}/approve')
			.mutationOptions,
		onSuccess: decided,
	})
	const reject = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/drafts/{assessment_id}/reject')
			.mutationOptions,
		onSuccess: decided,
	})

	const draftForm = useForm({
		defaultValues: valuesFromEntry(entry),
		onSubmit: ({ value }) => {
			edit.mutate({
				path: { assessment_id: assessmentId },
				body: { subject: value.subject, body: value.body },
			})
		},
	})
	const rejectForm = useForm({
		defaultValues: { reason: '' },
		onSubmit: ({ value }) => {
			const reason = value.reason.trim()
			reject.mutate({
				path: { assessment_id: assessmentId },
				body: reason === '' ? {} : { reason },
			})
		},
	})

	const values = useStore(draftForm.store, (state) => state.values)
	const reasonValue = useStore(rejectForm.store, (state) => state.values.reason)
	const dirty =
		values.subject !== entry.draft.subject || values.body !== entry.draft.body

	const editError = edit.isError ? parseApiError(edit.error) : null
	const approveError = approve.isError ? parseApiError(approve.error) : null
	const rejectError = reject.isError ? parseApiError(reject.error) : null
	const decisionError = approveError?.message ?? null

	const editor = (
		<DraftEditor
			values={values}
			fieldErrors={{
				subject: editError?.fields.subject,
				body: editError?.fields.body,
			}}
			formError={
				editError && Object.keys(editError.fields).length === 0
					? editError.message
					: null
			}
			dirty={dirty}
			saving={edit.isPending}
			onChange={(field, value) => {
				if (edit.isError) edit.reset()
				draftForm.setFieldValue(field, value)
			}}
			onSave={() => void draftForm.handleSubmit()}
		/>
	)

	const rejectPanel = (
		<RejectForm
			reason={reasonValue}
			reasonError={rejectError?.fields.reason ?? null}
			formError={
				rejectError && Object.keys(rejectError.fields).length === 0
					? rejectError.message
					: null
			}
			rejecting={reject.isPending}
			onReasonChange={(reason) => {
				if (reject.isError) reject.reset()
				rejectForm.setFieldValue('reason', reason)
			}}
			onConfirm={() => void rejectForm.handleSubmit()}
			onCancel={() => {
				reject.reset()
				rejectForm.reset()
				setRejectOpen(false)
			}}
		/>
	)

	return (
		<DraftReview
			state="ready"
			errorMessage={null}
			entry={entry}
			editor={editor}
			rejectForm={rejectPanel}
			rejectOpen={rejectOpen}
			dirty={dirty}
			deciding={approve.isPending || reject.isPending}
			decisionError={decisionError}
			onBack={() => void navigate({ to: '/drafts', search: {} })}
			onOpenLead={(leadId) =>
				void navigate({ to: '/leads/$leadId', params: { leadId } })
			}
			onApprove={() => {
				approve.mutate({ path: { assessment_id: assessmentId } })
			}}
			onStartReject={() => setRejectOpen(true)}
		/>
	)
}
