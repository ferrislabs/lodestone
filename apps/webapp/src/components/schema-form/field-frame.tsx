import type { ReactNode } from 'react'

import { Label } from '#/components/ui/label'
import type { Problem } from '#/lib/schema-form'

interface FieldFrameProps {
	id: string
	label: string
	description: string | undefined
	required: boolean
	problems: readonly Problem[]
	container?: boolean
	children: ReactNode
}

export function RequiredMark() {
	return (
		<span className="text-destructive" title="Required">
			*
		</span>
	)
}

export function FieldErrors({
	id,
	problems,
}: {
	id: string
	problems: readonly Problem[]
}) {
	if (problems.length === 0) return null
	return (
		<ul id={`${id}-error`} className="flex flex-col gap-0.5">
			{problems.map((problem) => (
				<li
					key={problem.message}
					role="alert"
					className="text-sm text-destructive"
				>
					{problem.message}
				</li>
			))}
		</ul>
	)
}

export function FieldFrame({
	id,
	label,
	description,
	required,
	problems,
	container = false,
	children,
}: FieldFrameProps) {
	return (
		<div className="flex flex-col gap-1.5">
			{container ? (
				<p className="text-sm font-semibold">
					{label} {required ? <RequiredMark /> : null}
				</p>
			) : (
				<Label htmlFor={id}>
					{label} {required ? <RequiredMark /> : null}
				</Label>
			)}
			{description ? (
				<p className="text-xs text-muted-foreground">{description}</p>
			) : null}
			{children}
			<FieldErrors id={id} problems={problems} />
		</div>
	)
}
