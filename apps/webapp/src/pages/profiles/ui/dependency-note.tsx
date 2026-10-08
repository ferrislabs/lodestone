import type { DependencyNote } from '#/pages/profiles/lib/agents'

export function DependencyNoteText({ note }: { note: DependencyNote }) {
	return (
		<p
			role={note.warn ? 'alert' : undefined}
			className={
				note.warn
					? 'rounded-md border border-amber-500/50 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-400'
					: 'text-xs text-muted-foreground'
			}
		>
			{note.text}
			{note.warn
				? " Aucun agent de ce type dans ce profil pour l'instant."
				: ''}
		</p>
	)
}
