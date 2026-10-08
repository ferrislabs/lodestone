import { ExternalLink } from 'lucide-react'

import { Badge } from '#/components/ui/badge'
import type { FindingRow } from '#/lib/ecosystem'
import { ProvenanceLink } from '#/pages/leads/ui/provenance-link'

interface FindingListProps {
	rows: readonly FindingRow[]
	empty: string
}

function metaOf(row: FindingRow): string[] {
	return [row.date, row.amount, ...row.facts].filter(
		(part): part is string => part !== null,
	)
}

export function FindingList({ rows, empty }: FindingListProps) {
	if (rows.length === 0) {
		return <p className="text-sm text-muted-foreground">{empty}</p>
	}
	return (
		<ul className="flex flex-col gap-2">
			{rows.map((row) => {
				const meta = metaOf(row)
				return (
					<li
						key={row.key}
						data-finding={row.key}
						className="flex min-w-0 flex-col gap-0.5"
					>
						<div className="flex flex-wrap items-center gap-2 text-sm">
							<span className="text-muted-foreground">{row.label}</span>
							<span className="font-medium">{row.value}</span>
							{row.badge ? (
								<Badge
									variant="outline"
									data-badge="collective"
									className="border-red-200 bg-red-50 text-red-700"
								>
									{row.badge}
								</Badge>
							) : null}
							{row.highlighted ? (
								<Badge
									variant="outline"
									className="border-orange-200 bg-orange-50 text-orange-700"
								>
									Mis en avant
								</Badge>
							) : null}
						</div>
						{meta.length > 0 ? (
							<span
								data-finding-meta={row.key}
								className="text-xs text-muted-foreground"
							>
								{meta.join(' · ')}
							</span>
						) : null}
						{row.link ? (
							<a
								href={row.link}
								target="_blank"
								rel="noopener noreferrer"
								className="inline-flex items-center gap-1 text-xs text-orange-600 hover:underline"
							>
								Voir l'annonce
								<ExternalLink className="size-3 shrink-0" />
							</a>
						) : null}
						<ProvenanceLink provenance={row.sourceUrl} />
					</li>
				)
			})}
		</ul>
	)
}
