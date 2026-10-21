import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { formatNumber } from '#/lib/format'
import type { Quality } from '#/pages/quality/lib/types'

interface DecisionsCardProps {
	decisions: Quality['decisions']
}

export function DecisionsCard({ decisions }: DecisionsCardProps) {
	const items = [
		{ label: 'Leads validés', value: decisions.validated },
		{ label: 'Leads rejetés', value: decisions.rejected },
		{ label: 'Brouillons approuvés', value: decisions.drafts_approved },
		{ label: 'Brouillons rejetés', value: decisions.drafts_rejected },
	]
	return (
		<Card aria-label="Décisions">
			<CardHeader>
				<CardTitle>Décisions</CardTitle>
			</CardHeader>
			<CardContent>
				<dl className="grid grid-cols-2 gap-4 md:grid-cols-4">
					{items.map((item) => (
						<div key={item.label} className="flex flex-col gap-0.5">
							<dt className="text-sm text-muted-foreground">{item.label}</dt>
							<dd className="text-2xl font-semibold tabular-nums">
								{formatNumber(item.value)}
							</dd>
						</div>
					))}
				</dl>
			</CardContent>
		</Card>
	)
}
