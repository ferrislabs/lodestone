import { Button } from '#/components/ui/button'
import { Label } from '#/components/ui/label'
import { Textarea } from '#/components/ui/textarea'

interface RejectFormProps {
	reason: string
	reasonError: string | null
	formError: string | null
	rejecting: boolean
	onReasonChange: (reason: string) => void
	onConfirm: () => void
	onCancel: () => void
}

export function RejectForm({
	reason,
	reasonError,
	formError,
	rejecting,
	onReasonChange,
	onConfirm,
	onCancel,
}: RejectFormProps) {
	return (
		<form
			className="flex flex-col gap-3 rounded-lg border border-destructive/30 p-4"
			onSubmit={(event) => {
				event.preventDefault()
				onConfirm()
			}}
		>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="reject-reason">Reason (optional)</Label>
				<Textarea
					id="reject-reason"
					rows={3}
					value={reason}
					aria-invalid={reasonError !== null}
					aria-describedby={reasonError ? 'reject-reason-error' : undefined}
					onChange={(event) => onReasonChange(event.target.value)}
				/>
				{reasonError ? (
					<p
						id="reject-reason-error"
						role="alert"
						className="text-sm text-destructive"
					>
						{reasonError}
					</p>
				) : null}
			</div>
			{formError ? (
				<p role="alert" className="text-sm text-destructive">
					{formError}
				</p>
			) : null}
			<div className="flex gap-2">
				<Button type="submit" variant="destructive" disabled={rejecting}>
					{rejecting ? 'Rejecting...' : 'Confirm rejection'}
				</Button>
				<Button type="button" variant="ghost" onClick={onCancel}>
					Cancel
				</Button>
			</div>
		</form>
	)
}
