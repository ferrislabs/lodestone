import { Badge } from '#/components/ui/badge'
import type { FindingRow } from '#/lib/ecosystem'
import { ProvenanceLink } from '#/pages/leads/ui/provenance-link'

interface FindingListProps {
	rows: readonly FindingRow[]
	empty: string
}

export function FindingList({ rows, empty }: FindingListProps) {
	if (rows.length === 0) {
		return <p className="text-sm text-muted-foreground">{empty}</p>
	}
	return (
		<ul className="flex flex-col gap-2">
			{rows.map((row) => (
				<li
					key={row.key}
					data-finding={row.key}
					className="flex min-w-0 flex-col gap-0.5"
				>
					<div className="flex flex-wrap items-center gap-2 text-sm">
						<span className="text-muted-foreground">{row.label}</span>
						<span className="font-medium">{row.value}</span>
						{row.highlighted ? (
							<Badge
								variant="outline"
								className="border-orange-200 bg-orange-50 text-orange-700"
							>
								Mis en avant
							</Badge>
						) : null}
					</div>
					<ProvenanceLink provenance={row.sourceUrl} />
				</li>
			))}
		</ul>
	)
}
