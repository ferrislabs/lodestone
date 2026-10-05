import type { ReactNode } from 'react'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'

interface ProfileNewProps {
	name: string
	nameError: string | null
	formError: string | null
	tabs: ReactNode
	editor: ReactNode
	onNameChange: (name: string) => void
	onBack: () => void
}

export function ProfileNew({
	name,
	nameError,
	formError,
	tabs,
	editor,
	onNameChange,
	onBack,
}: ProfileNewProps) {
	return (
		<div className="flex flex-col gap-4 p-4 md:p-8">
			<div>
				<Button variant="ghost" size="sm" onClick={onBack}>
					Back to profiles
				</Button>
			</div>
			<h1 className="text-2xl font-bold tracking-tight">New profile</h1>
			<div className="flex max-w-md flex-col gap-1.5">
				<Label htmlFor="profile-name">Name</Label>
				<Input
					id="profile-name"
					value={name}
					placeholder="Defaults to the offer"
					aria-invalid={nameError !== null}
					aria-describedby={nameError ? 'profile-name-error' : undefined}
					onChange={(event) => onNameChange(event.target.value)}
				/>
				{nameError ? (
					<p
						id="profile-name-error"
						role="alert"
						className="text-sm text-destructive"
					>
						{nameError}
					</p>
				) : null}
			</div>
			{formError ? (
				<p role="alert" className="text-sm text-destructive">
					{formError}
				</p>
			) : null}
			{tabs}
			{editor}
		</div>
	)
}
