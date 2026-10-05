import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { Textarea } from '#/components/ui/textarea'
import type { DraftValues } from '#/pages/drafts/lib/types'

interface DraftEditorProps {
	values: DraftValues
	fieldErrors: Partial<Record<keyof DraftValues, string>>
	formError: string | null
	dirty: boolean
	saving: boolean
	onChange: (field: keyof DraftValues, value: string) => void
	onSave: () => void
}

export function DraftEditor({
	values,
	fieldErrors,
	formError,
	dirty,
	saving,
	onChange,
	onSave,
}: DraftEditorProps) {
	return (
		<form
			className="flex flex-col gap-4"
			onSubmit={(event) => {
				event.preventDefault()
				onSave()
			}}
		>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="draft-subject">Objet</Label>
				<Input
					id="draft-subject"
					value={values.subject}
					aria-invalid={fieldErrors.subject !== undefined}
					aria-describedby={
						fieldErrors.subject ? 'draft-subject-error' : undefined
					}
					onChange={(event) => onChange('subject', event.target.value)}
				/>
				{fieldErrors.subject ? (
					<p
						id="draft-subject-error"
						role="alert"
						className="text-sm text-destructive"
					>
						{fieldErrors.subject}
					</p>
				) : null}
			</div>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="draft-body">Corps</Label>
				<Textarea
					id="draft-body"
					rows={14}
					value={values.body}
					aria-invalid={fieldErrors.body !== undefined}
					aria-describedby={fieldErrors.body ? 'draft-body-error' : undefined}
					onChange={(event) => onChange('body', event.target.value)}
				/>
				{fieldErrors.body ? (
					<p
						id="draft-body-error"
						role="alert"
						className="text-sm text-destructive"
					>
						{fieldErrors.body}
					</p>
				) : null}
			</div>
			{formError ? (
				<p role="alert" className="text-sm text-destructive">
					{formError}
				</p>
			) : null}
			<div className="flex items-center gap-3">
				<Button type="submit" disabled={!dirty || saving}>
					{saving ? 'Enregistrement…' : 'Enregistrer les modifications'}
				</Button>
				{dirty ? (
					<span className="text-sm text-amber-600">
						Modifications non enregistrées
					</span>
				) : null}
			</div>
		</form>
	)
}
