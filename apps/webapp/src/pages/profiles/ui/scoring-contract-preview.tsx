import type { JsonObject } from '#/lib/schema-form'

interface ScoringContractPreviewProps {
	imposed: JsonObject
	criteria: JsonObject
}

export function ScoringContractPreview({
	imposed,
	criteria,
}: ScoringContractPreviewProps) {
	return (
		<div className="flex flex-col gap-3">
			<div className="flex flex-col gap-1">
				<p className="text-xs font-medium">
					Partie imposée par l'application (lecture seule)
				</p>
				<p className="text-xs text-muted-foreground">
					Le score de 0 à 100, le verdict avec ses probabilités et les raisons
					ne se configurent pas : ils garantissent que les agents de scoring se
					combinent.
				</p>
				<pre
					data-testid="contract-imposed"
					className="max-h-72 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs"
				>
					{JSON.stringify(imposed, null, 2)}
				</pre>
			</div>
			<div className="flex flex-col gap-1">
				<p className="text-xs font-medium">
					Critères (générés à partir de vos critères)
				</p>
				<pre
					data-testid="contract-criteria"
					className="max-h-72 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs"
				>
					{JSON.stringify(criteria, null, 2)}
				</pre>
			</div>
		</div>
	)
}
