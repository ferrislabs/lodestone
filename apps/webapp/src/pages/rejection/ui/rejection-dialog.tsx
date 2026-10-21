import { Button } from '#/components/ui/button'
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from '#/components/ui/dialog'
import { Label } from '#/components/ui/label'
import { Textarea } from '#/components/ui/textarea'
import {
	REJECTION_NOTE_MAX,
	REJECTION_REASONS,
	type RejectionErrors,
	type RejectionReason,
	rejectionReasonLabel,
} from '#/pages/rejection/lib/reasons'

interface RejectionDialogProps {
	open: boolean
	title: string
	description: string
	reason: RejectionReason | null
	note: string
	errors: RejectionErrors
	formError: string | null
	pending: boolean
	onReasonChange: (reason: RejectionReason) => void
	onNoteChange: (note: string) => void
	onConfirm: () => void
	onCancel: () => void
}

export function RejectionDialog({
	open,
	title,
	description,
	reason,
	note,
	errors,
	formError,
	pending,
	onReasonChange,
	onNoteChange,
	onConfirm,
	onCancel,
}: RejectionDialogProps) {
	return (
		<Dialog
			open={open}
			onOpenChange={(next) => {
				if (!next) onCancel()
			}}
		>
			<DialogContent>
				<form
					className="flex flex-col gap-4"
					onSubmit={(event) => {
						event.preventDefault()
						onConfirm()
					}}
				>
					<DialogHeader>
						<DialogTitle>{title}</DialogTitle>
						<DialogDescription>{description}</DialogDescription>
					</DialogHeader>
					<fieldset className="flex flex-col gap-2">
						<legend className="mb-1 text-sm font-medium">
							Raison du rejet
						</legend>
						{REJECTION_REASONS.map((value) => (
							<label
								key={value}
								className="flex cursor-pointer items-start gap-2 text-sm"
							>
								<input
									type="radio"
									name="rejection-reason"
									value={value}
									checked={reason === value}
									className="mt-0.5"
									onChange={() => onReasonChange(value)}
								/>
								{rejectionReasonLabel(value)}
							</label>
						))}
						{errors.reason ? (
							<p role="alert" className="text-sm text-destructive">
								{errors.reason}
							</p>
						) : null}
					</fieldset>
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="rejection-note">
							{reason === 'autre' ? 'Précision' : 'Précision (facultatif)'}
						</Label>
						<Textarea
							id="rejection-note"
							rows={3}
							maxLength={REJECTION_NOTE_MAX * 2}
							value={note}
							aria-invalid={errors.note !== null}
							aria-describedby={
								errors.note ? 'rejection-note-error' : undefined
							}
							onChange={(event) => onNoteChange(event.target.value)}
						/>
						{errors.note ? (
							<p
								id="rejection-note-error"
								role="alert"
								className="text-sm text-destructive"
							>
								{errors.note}
							</p>
						) : null}
					</div>
					{formError ? (
						<p role="alert" className="text-sm text-destructive">
							{formError}
						</p>
					) : null}
					<DialogFooter>
						<Button type="button" variant="ghost" onClick={onCancel}>
							Annuler
						</Button>
						<Button type="submit" variant="destructive" disabled={pending}>
							{pending ? 'Rejet en cours…' : 'Confirmer le rejet'}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	)
}
