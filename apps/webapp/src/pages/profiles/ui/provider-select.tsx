import type { ProviderOption } from '#/pages/profiles/lib/qualification'

const SELECT_CLASS =
	'h-9 w-full rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'

interface ProviderSelectProps {
	id: string
	value: string
	providers: readonly ProviderOption[]
	invalid?: boolean
	emptyLabel?: string
	onChange: (providerId: string) => void
}

export function ProviderSelect({
	id,
	value,
	providers,
	invalid = false,
	emptyLabel = 'Fournisseur par défaut',
	onChange,
}: ProviderSelectProps) {
	const known = value === '' || providers.some((item) => item.id === value)
	return (
		<select
			id={id}
			className={SELECT_CLASS}
			value={value}
			aria-invalid={invalid}
			onChange={(event) => onChange(event.target.value)}
		>
			<option value="">{emptyLabel}</option>
			{known ? null : <option value={value}>Fournisseur introuvable</option>}
			{providers.map((provider) => (
				<option key={provider.id} value={provider.id}>
					{provider.name}
					{provider.enabled ? '' : ' (désactivé)'}
				</option>
			))}
		</select>
	)
}
