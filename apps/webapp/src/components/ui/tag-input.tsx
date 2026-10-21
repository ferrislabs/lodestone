import type { ClipboardEvent, KeyboardEvent } from 'react'
import { mergeTags, splitTags } from '#/lib/tags'
import { cn } from '#/lib/utils'

interface TagInputProps {
	id: string
	label: string
	values: readonly string[]
	invalid?: readonly number[]
	describedBy?: string
	placeholder?: string
	onChange: (next: string[]) => void
}

function flagDuplicate(input: HTMLInputElement, duplicate: boolean) {
	const root = input.closest<HTMLElement>('[data-tag-input]')
	if (!root) return
	if (duplicate) root.dataset.dup = 'true'
	else delete root.dataset.dup
}

export function TagInput({
	id,
	label,
	values,
	invalid = [],
	describedBy,
	placeholder = 'Saisir puis Entrée',
	onChange,
}: TagInputProps) {
	const keys = values.map((value, index) => `${index}-${value}`)
	const commit = (input: HTMLInputElement, incoming: readonly string[]) => {
		const merged = mergeTags(values, incoming)
		flagDuplicate(input, merged.duplicates > 0)
		input.value = ''
		if (merged.added > 0) onChange(merged.next)
	}

	const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
		const input = event.currentTarget
		if (event.key === 'Enter' || event.key === ',') {
			event.preventDefault()
			commit(input, [input.value])
			return
		}
		if (event.key === 'Tab' && !event.shiftKey && input.value.trim() !== '') {
			event.preventDefault()
			commit(input, [input.value])
			return
		}
		if (event.key === 'Backspace' && input.value === '' && values.length > 0) {
			onChange(values.slice(0, -1))
		}
	}

	const onPaste = (event: ClipboardEvent<HTMLInputElement>) => {
		const text = event.clipboardData.getData('text')
		if (!/[,;\n\r]/.test(text)) return
		event.preventDefault()
		commit(event.currentTarget, splitTags(text))
	}

	return (
		<div data-tag-input className="group/tags flex flex-col gap-1">
			<div
				className={cn(
					'flex min-h-9 flex-wrap items-center gap-1.5 rounded-md border border-input bg-transparent px-2 py-1.5 text-sm shadow-xs focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50 dark:bg-input/30',
					invalid.length > 0 && 'border-destructive',
				)}
			>
				{values.map((value, index) => {
					const bad = invalid.includes(index)
					return (
						<span
							key={keys[index]}
							data-tag={index}
							aria-invalid={bad}
							className={cn(
								'inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs font-medium',
								bad
									? 'border-destructive bg-destructive/10 text-destructive'
									: 'bg-secondary text-secondary-foreground',
							)}
						>
							{value}
							<button
								type="button"
								aria-label={`Retirer ${value}`}
								className="rounded-sm px-0.5 leading-none opacity-70 hover:opacity-100 focus-visible:ring-2 focus-visible:ring-ring"
								onClick={() =>
									onChange(values.filter((_, position) => position !== index))
								}
							>
								×
							</button>
						</span>
					)
				})}
				<input
					id={id}
					aria-label={label}
					aria-invalid={invalid.length > 0}
					aria-describedby={describedBy}
					placeholder={values.length === 0 ? placeholder : undefined}
					autoComplete="off"
					className="min-w-24 flex-1 bg-transparent py-0.5 outline-none placeholder:text-muted-foreground"
					onKeyDown={onKeyDown}
					onPaste={onPaste}
					onChange={(event) => flagDuplicate(event.currentTarget, false)}
					onBlur={(event) => {
						if (event.currentTarget.value.trim() !== '') {
							commit(event.currentTarget, [event.currentTarget.value])
						}
					}}
				/>
			</div>
			<p
				role="status"
				className="hidden text-xs text-muted-foreground group-data-[dup=true]/tags:block"
			>
				Déjà présent, ignoré.
			</p>
		</div>
	)
}
