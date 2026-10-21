import { useForm, useStore } from '@tanstack/react-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { type ParsedApiError, parseApiError } from '#/lib/api-errors'
import { CONFLICT_MESSAGE, isConflict } from '#/pages/drafts/lib/conflict'
import { valuesFromEntry } from '#/pages/drafts/lib/edit'
import type { DraftEntry } from '#/pages/drafts/lib/types'
import { DraftEditor } from '#/pages/drafts/ui/draft-editor'
import { DraftReview } from '#/pages/drafts/ui/draft-review'
import { useRejectionForm } from '#/pages/rejection/feature/use-rejection-form'
import { RejectionDialog } from '#/pages/rejection/ui/rejection-dialog'

function readError(error: unknown): ParsedApiError {
	const parsed = parseApiError(error)
	return isConflict(parsed)
		? { ...parsed, message: CONFLICT_MESSAGE, fields: {} }
		: parsed
}

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
			onCopy={() => {}}
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
	const rejection = useRejectionForm()

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

	const refreshOnConflict = async (error: unknown) => {
		if (isConflict(parseApiError(error))) await invalidate()
	}

	const edit = useMutation({
		...lodestoneApi().mutation('patch', '/api/v1/drafts/{assessment_id}')
			.mutationOptions,
		onError: refreshOnConflict,
		onSuccess: async (response) => {
			draftForm.reset(valuesFromEntry(response.data))
			await invalidate()
		},
	})
	const approve = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/drafts/{assessment_id}/approve')
			.mutationOptions,
		onError: refreshOnConflict,
		onSuccess: decided,
	})
	const reject = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/drafts/{assessment_id}/reject')
			.mutationOptions,
		onError: refreshOnConflict,
		onSuccess: decided,
	})

	const draftForm = useForm({
		defaultValues: valuesFromEntry(entry),
		onSubmit: ({ value }) => {
			edit.mutate({
				path: { assessment_id: assessmentId },
				body: {
					subject: value.subject,
					body: value.body,
					expected_revision: entry.draft.revision,
				},
			})
		},
	})

	const values = useStore(draftForm.store, (state) => state.values)
	const dirty =
		values.subject !== entry.draft.subject || values.body !== entry.draft.body

	const editError = edit.isError ? readError(edit.error) : null
	const approveError = approve.isError ? readError(approve.error) : null
	const rejectError = reject.isError ? readError(reject.error) : null
	const decisionError = approveError?.message ?? null

	const copy = (text: string) => {
		void navigator.clipboard?.writeText(text)
	}

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
			contactAddress={entry.contact?.address ?? null}
			onChange={(field, value) => {
				if (edit.isError) edit.reset()
				draftForm.setFieldValue(field, value)
			}}
			onSave={() => void draftForm.handleSubmit()}
			onCopy={copy}
		/>
	)

	const rejectPanel = (
		<RejectionDialog
			open={rejection.open}
			title="Rejeter ce brouillon"
			description="Le lead est rejeté avec lui. La raison est conservée avec la décision pour mesurer la qualité des verdicts."
			reason={rejection.reason}
			note={rejection.note}
			errors={{
				reason: rejection.errors.reason ?? rejectError?.fields.reason ?? null,
				note: rejection.errors.note ?? rejectError?.fields.note ?? null,
			}}
			formError={
				rejectError && Object.keys(rejectError.fields).length === 0
					? rejectError.message
					: null
			}
			pending={reject.isPending}
			onReasonChange={(reason) => {
				reject.reset()
				rejection.setReason(reason)
			}}
			onNoteChange={(note) => {
				reject.reset()
				rejection.setNote(note)
			}}
			onConfirm={() =>
				rejection.submit((payload) =>
					reject.mutate({
						path: { assessment_id: assessmentId },
						body: { ...payload, expected_revision: entry.draft.revision },
					}),
				)
			}
			onCancel={() => {
				reject.reset()
				rejection.close()
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
			rejectOpen={rejection.open}
			dirty={dirty}
			deciding={approve.isPending || reject.isPending}
			decisionError={decisionError}
			onBack={() => void navigate({ to: '/drafts', search: {} })}
			onOpenLead={(leadId) =>
				void navigate({ to: '/leads/$leadId', params: { leadId } })
			}
			onApprove={() => {
				approve.mutate({
					path: { assessment_id: assessmentId },
					body: { expected_revision: entry.draft.revision },
				})
			}}
			onStartReject={() => rejection.start()}
			onCopy={copy}
		/>
	)
}
