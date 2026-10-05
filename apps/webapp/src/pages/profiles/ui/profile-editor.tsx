import { Button } from '#/components/ui/button'
import { Label } from '#/components/ui/label'
import { Textarea } from '#/components/ui/textarea'
import type { Problem } from '#/pages/profiles/lib/types'
import { ProfileProblems } from '#/pages/profiles/ui/profile-problems'

interface ProfileEditorProps {
	text: string
	problems: readonly Problem[]
	notice: string | null
	dirty: boolean
	saving: boolean
	saveLabel: string
	onTextChange: (text: string) => void
	onSave: () => void
}

export function ProfileEditor({
	text,
	problems,
	notice,
	dirty,
	saving,
	saveLabel,
	onTextChange,
	onSave,
}: ProfileEditorProps) {
	return (
		<form
			className="flex flex-col gap-3"
			onSubmit={(event) => {
				event.preventDefault()
				onSave()
			}}
		>
			<ProfileProblems problems={problems} />
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="profile-definition">Definition (JSON)</Label>
				<Textarea
					id="profile-definition"
					spellCheck={false}
					rows={28}
					className="font-mono text-xs"
					value={text}
					aria-invalid={problems.length > 0}
					onChange={(event) => onTextChange(event.target.value)}
				/>
			</div>
			{notice ? (
				<output className="block text-sm text-muted-foreground">
					{notice}
				</output>
			) : null}
			<div className="flex items-center gap-3">
				<Button type="submit" disabled={saving || !dirty}>
					{saving ? 'Saving...' : saveLabel}
				</Button>
				{dirty ? (
					<span className="text-xs text-muted-foreground">Unsaved changes</span>
				) : null}
			</div>
		</form>
	)
}
