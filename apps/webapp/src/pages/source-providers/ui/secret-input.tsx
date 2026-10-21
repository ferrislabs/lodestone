import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import type {
	ProviderField,
	SecretState,
} from '#/pages/source-providers/lib/types'

interface SecretInputProps {
	field: ProviderField
	state: SecretState | undefined
	replacing: boolean
	value: string
	error: string | undefined
	onChange: (key: string, value: string) => void
	onReplace: (key: string) => void
	onCancel: (key: string) => void
}

export function SecretInput({
	field,
	state,
	replacing,
	value,
	error,
	onChange,
	onReplace,
	onCancel,
}: SecretInputProps) {
	const id = `secret-${field.key}`
	const isSet = state?.is_set === true
	const label = field.required ? `${field.label} *` : field.label
	return (
		<div className="flex flex-col gap-1.5">
			<Label htmlFor={id}>{label}</Label>
			{isSet && !replacing ? (
				<div className="flex items-center gap-2">
					<Input
						id={id}
						readOnly
						value={state?.hint ?? '••••'}
						aria-label={`${field.label} (masqué)`}
						className="font-mono"
					/>
					<Button
						type="button"
						variant="outline"
						onClick={() => onReplace(field.key)}
					>
						Remplacer
					</Button>
				</div>
			) : (
				<div className="flex items-center gap-2">
					<Input
						id={id}
						type="password"
						autoComplete="new-password"
						value={value}
						aria-invalid={error !== undefined}
						onChange={(event) => onChange(field.key, event.target.value)}
					/>
					{isSet ? (
						<Button
							type="button"
							variant="ghost"
							onClick={() => onCancel(field.key)}
						>
							Annuler
						</Button>
					) : null}
				</div>
			)}
			{field.help ? (
				<p className="text-xs text-muted-foreground">{field.help}</p>
			) : null}
			{error ? (
				<p role="alert" className="text-sm text-destructive">
					{error}
				</p>
			) : null}
		</div>
	)
}
