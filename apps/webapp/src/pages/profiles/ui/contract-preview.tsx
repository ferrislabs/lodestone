import type { JsonObject } from '#/lib/schema-form'

interface ContractPreviewProps {
	schema: JsonObject
}

export function ContractPreview({ schema }: ContractPreviewProps) {
	return (
		<div className="flex flex-col gap-1">
			<p className="text-xs text-muted-foreground">
				JSON Schema généré à partir des questions, en lecture seule.
			</p>
			<pre
				data-testid="contract-preview"
				className="max-h-96 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs"
			>
				{JSON.stringify(schema, null, 2)}
			</pre>
		</div>
	)
}
