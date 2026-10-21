import type { ReactNode } from 'react'

import { Button } from '#/components/ui/button'
import type { Problem } from '#/pages/profiles/lib/types'
import { ProfileProblems } from '#/pages/profiles/ui/profile-problems'

interface ProfileEditorProps {
	form: ReactNode
	problems: readonly Problem[]
	notice: string | null
	dirty: boolean
	saving: boolean
	saveLabel: string
	blockedReason?: string | null
	onSave: () => void
}

export function ProfileEditor({
	form,
	problems,
	notice,
	dirty,
	saving,
	saveLabel,
	blockedReason = null,
	onSave,
}: ProfileEditorProps) {
	return (
		<form
			noValidate
			className="flex flex-col gap-3"
			onSubmit={(event) => {
				event.preventDefault()
				onSave()
			}}
		>
			<ProfileProblems
				problems={problems}
				title="Problems not tied to a field"
			/>
			{form}
			{notice ? (
				<output className="block text-sm text-muted-foreground">
					{notice}
				</output>
			) : null}
			<div className="flex items-center gap-3">
				<Button
					type="submit"
					disabled={saving || !dirty || blockedReason !== null}
				>
					{saving ? 'Saving...' : saveLabel}
				</Button>
				{blockedReason ? (
					<span className="text-xs text-muted-foreground">{blockedReason}</span>
				) : dirty ? (
					<span className="text-xs text-muted-foreground">Unsaved changes</span>
				) : null}
			</div>
		</form>
	)
}
