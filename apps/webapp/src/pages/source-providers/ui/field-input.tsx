import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { Switch } from '#/components/ui/switch'
import type { ProviderField } from '#/pages/source-providers/lib/types'

const SELECT_CLASS =
	'h-9 w-full rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'

interface FieldInputProps {
	field: ProviderField
	value: string | boolean
	error: string | undefined
	onChange: (key: string, value: string | boolean) => void
}

function describedBy(field: ProviderField, error: string | undefined) {
	const ids = [
		field.help ? `field-${field.key}-help` : null,
		error ? `field-${field.key}-error` : null,
	].filter((id) => id !== null)
	return ids.length === 0 ? undefined : ids.join(' ')
}

export function FieldInput({ field, value, error, onChange }: FieldInputProps) {
	const id = `field-${field.key}`
	const label = field.required ? `${field.label} *` : field.label
	return (
		<div className="flex flex-col gap-1.5">
			<Label htmlFor={id}>{label}</Label>
			{field.kind === 'bool' ? (
				<Switch
					id={id}
					checked={value === true}
					onCheckedChange={(checked) => onChange(field.key, checked)}
				/>
			) : field.kind === 'select' ? (
				<select
					id={id}
					className={SELECT_CLASS}
					value={String(value)}
					aria-invalid={error !== undefined}
					aria-describedby={describedBy(field, error)}
					onChange={(event) => onChange(field.key, event.target.value)}
				>
					{field.required ? null : <option value="">Aucune</option>}
					{field.options.map((option) => (
						<option key={option} value={option}>
							{option}
						</option>
					))}
				</select>
			) : (
				<Input
					id={id}
					value={String(value)}
					inputMode={field.kind === 'number' ? 'decimal' : undefined}
					placeholder={field.default ?? undefined}
					aria-invalid={error !== undefined}
					aria-describedby={describedBy(field, error)}
					onChange={(event) => onChange(field.key, event.target.value)}
				/>
			)}
			{field.help ? (
				<p id={`${id}-help`} className="text-xs text-muted-foreground">
					{field.help}
				</p>
			) : null}
			{error ? (
				<p id={`${id}-error`} role="alert" className="text-sm text-destructive">
					{error}
				</p>
			) : null}
		</div>
	)
}
