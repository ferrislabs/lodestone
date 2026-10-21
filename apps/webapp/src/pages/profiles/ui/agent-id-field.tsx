import { FieldErrors } from '#/components/schema-form/field-frame'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import type { Problem } from '#/lib/schema-form'

interface AgentIdFieldProps {
	id: string
	value: string
	problems: readonly Problem[]
	onChange: (id: string) => void
}

export function AgentIdField({
	id,
	value,
	problems,
	onChange,
}: AgentIdFieldProps) {
	return (
		<div className="flex flex-col gap-1.5">
			<Label htmlFor={id}>Identifiant</Label>
			<p className="text-xs text-muted-foreground">
				Identifiant technique : minuscules, chiffres et tirets. Il suit le nom
				tant que vous ne le modifiez pas.
			</p>
			<Input
				id={id}
				className="font-mono text-xs"
				value={value}
				aria-invalid={problems.length > 0}
				onChange={(event) => onChange(event.target.value)}
			/>
			<FieldErrors id={id} problems={problems} />
		</div>
	)
}
