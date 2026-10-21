import type { RecentView } from '#/lib/ecosystem-views'
import { ProvenanceLink } from '#/pages/leads/ui/provenance-link'

interface RecentItemsProps {
	recent: RecentView
}

export function RecentItems({ recent }: RecentItemsProps) {
	return (
		<div className="flex flex-col gap-2" data-recent>
			{recent.freshness ? (
				<span data-freshness className="text-sm font-medium">
					{recent.freshness}
				</span>
			) : null}
			{recent.latest ? (
				<span data-latest className="text-sm">
					<span className="text-muted-foreground">Dernière actualité : </span>
					{recent.latest}
				</span>
			) : null}
			{recent.items.length > 0 ? (
				<ol className="flex flex-col gap-1.5">
					{recent.items.map((item) => (
						<li
							key={item.key}
							data-recent-item={item.url}
							className="flex min-w-0 flex-col gap-0.5"
						>
							<div className="flex flex-wrap items-baseline gap-2 text-sm">
								<span className="font-medium">{item.title}</span>
								{item.date ? (
									<span className="text-xs text-muted-foreground">
										{item.date}
									</span>
								) : null}
							</div>
							<ProvenanceLink provenance={item.url} />
						</li>
					))}
				</ol>
			) : null}
		</div>
	)
}
