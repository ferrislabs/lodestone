import { Button } from '#/components/ui/button'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { RevisionSummary } from '#/pages/profiles/lib/types'

interface RevisionHistoryProps {
	revisions: readonly RevisionSummary[]
	selected: number | null
	selectedText: string | null
	loadingSelected: boolean
	selectedError: string | null
	onSelect: (number: number) => void
	onRestore: () => void
}

export function RevisionHistory({
	revisions,
	selected,
	selectedText,
	loadingSelected,
	selectedError,
	onSelect,
	onRestore,
}: RevisionHistoryProps) {
	return (
		<div className="grid gap-4 lg:grid-cols-[20rem_1fr]">
			<ul className="flex flex-col divide-y rounded-xl border bg-card">
				{[...revisions].reverse().map((revision) => (
					<li key={revision.number}>
						<button
							type="button"
							aria-current={selected === revision.number}
							className="flex w-full flex-col gap-0.5 px-3 py-2 text-left text-sm hover:bg-muted aria-[current=true]:bg-muted"
							onClick={() => onSelect(revision.number)}
						>
							<span className="font-medium">Révision {revision.number}</span>
							<span className="text-xs text-muted-foreground">
								{formatDateTime(revision.created_at)}
							</span>
							<span className="font-mono text-xs text-muted-foreground">
								{revision.created_by ?? 'auteur inconnu'}
							</span>
						</button>
					</li>
				))}
			</ul>
			<div className="flex flex-col gap-3">
				{selected === null ? (
					<p className="text-sm text-muted-foreground">
						Sélectionner une révision pour l'afficher.
					</p>
				) : null}
				{selected !== null && loadingSelected ? (
					<p className="text-sm text-muted-foreground">
						Chargement de la révision…
					</p>
				) : null}
				{selectedError ? (
					<p role="alert" className="text-sm text-destructive">
						{selectedError}
					</p>
				) : null}
				{selected !== null && selectedText !== null ? (
					<>
						<div className="flex items-center justify-between">
							<h3 className="text-sm font-semibold">
								Révision {selected} (lecture seule)
							</h3>
							<Button size="sm" variant="outline" onClick={onRestore}>
								Restaurer comme nouvelle révision
							</Button>
						</div>
						<pre className="max-h-[36rem] overflow-auto rounded-md border bg-muted/40 p-3 font-mono text-xs">
							{selectedText}
						</pre>
					</>
				) : null}
			</div>
		</div>
	)
}
