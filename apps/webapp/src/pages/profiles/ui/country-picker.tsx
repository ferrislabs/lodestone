import { XIcon } from 'lucide-react'

import { countryName, searchCountries } from '#/pages/profiles/lib/countries'

interface CountryPickerProps {
	id: string
	codes: readonly string[]
	query: string
	onQueryChange: (query: string) => void
	onAdd: (code: string) => void
	onRemove: (code: string) => void
}

export function CountryPicker({
	id,
	codes,
	query,
	onQueryChange,
	onAdd,
	onRemove,
}: CountryPickerProps) {
	const options = searchCountries(query, codes)
	const listId = `${id}-options`
	const searching = query.trim() !== ''
	return (
		<div className="flex flex-col gap-1.5">
			<label htmlFor={id} className="text-sm font-medium">
				Pays
			</label>
			<div className="relative">
				<div className="flex min-h-9 flex-wrap items-center gap-1.5 rounded-md border border-input px-2 py-1.5 text-sm shadow-xs focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50">
					{codes.map((code) => (
						<span
							key={code}
							data-country={code}
							className="inline-flex items-center gap-1 rounded-full border bg-secondary px-2 py-0.5 text-xs font-medium"
						>
							{countryName(code)}
							<button
								type="button"
								aria-label={`Retirer ${countryName(code)}`}
								className="rounded-full opacity-70 hover:opacity-100"
								onClick={() => onRemove(code)}
							>
								<XIcon className="size-3" />
							</button>
						</span>
					))}
					<input
						id={id}
						role="combobox"
						aria-expanded={options.length > 0}
						aria-controls={listId}
						aria-autocomplete="list"
						autoComplete="off"
						value={query}
						placeholder="Rechercher un pays…"
						className="min-w-32 flex-1 bg-transparent py-0.5 outline-none placeholder:text-muted-foreground"
						onChange={(event) => onQueryChange(event.target.value)}
						onKeyDown={(event) => {
							if (event.key === 'Enter') {
								event.preventDefault()
								const first = options[0]
								if (first) onAdd(first.code)
							}
							if (event.key === 'ArrowDown') {
								event.preventDefault()
								document
									.getElementById(listId)
									?.querySelector<HTMLElement>('[role="option"]')
									?.focus()
							}
							if (event.key === 'Escape' && searching) {
								event.preventDefault()
								onQueryChange('')
							}
						}}
					/>
				</div>
				{options.length > 0 ? (
					<div
						id={listId}
						role="listbox"
						aria-label="Pays correspondants"
						className="absolute z-20 mt-1 flex max-h-60 w-full flex-col overflow-y-auto rounded-md border bg-popover p-1 shadow-md"
					>
						{options.map((country) => (
							<div
								key={country.code}
								role="option"
								aria-selected={false}
								tabIndex={-1}
								data-country-option={country.code}
								className="cursor-pointer rounded-sm px-2 py-1.5 text-sm outline-none hover:bg-accent focus:bg-accent"
								onMouseDown={(event) => event.preventDefault()}
								onClick={() => onAdd(country.code)}
								onKeyDown={(event) => {
									if (event.key === 'Enter') {
										event.preventDefault()
										onAdd(country.code)
									}
									if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
										event.preventDefault()
										const sibling =
											event.key === 'ArrowDown'
												? event.currentTarget.nextElementSibling
												: event.currentTarget.previousElementSibling
										if (sibling instanceof HTMLElement) sibling.focus()
									}
									if (event.key === 'Escape') {
										event.preventDefault()
										document.getElementById(id)?.focus()
									}
								}}
							>
								{country.name}
							</div>
						))}
					</div>
				) : null}
			</div>
			{searching && options.length === 0 ? (
				<p className="text-xs text-muted-foreground">Aucun pays trouvé.</p>
			) : null}
			<p className="text-xs text-muted-foreground">
				Sans pays choisi, aucun filtre de pays.
			</p>
		</div>
	)
}
