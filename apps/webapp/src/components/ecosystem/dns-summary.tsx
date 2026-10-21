import type { DnsLine } from '#/lib/ecosystem-views'

interface DnsSummaryProps {
	lines: readonly DnsLine[]
}

export function DnsSummary({ lines }: DnsSummaryProps) {
	return (
		<dl className="flex flex-col gap-1.5" data-dns-summary>
			{lines.map((line) => (
				<div key={line.kind} data-dns={line.kind} className="flex flex-col">
					<dt className="text-xs text-muted-foreground">{line.title}</dt>
					<dd className="text-sm font-medium">{line.values.join(', ')}</dd>
				</div>
			))}
		</dl>
	)
}
