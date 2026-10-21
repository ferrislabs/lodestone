import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from '#/components/ui/card'
import { formatNumber } from '#/lib/format'
import { formatRatio } from '#/pages/quality/lib/format'
import type { Quality } from '#/pages/quality/lib/types'

interface DiscardedCardProps {
	discarded: Quality['discarded']
}

export function DiscardedCard({ discarded }: DiscardedCardProps) {
	return (
		<Card aria-label="Taux d'écartés">
			<CardHeader>
				<CardTitle>Taux d'écartés</CardTitle>
				<CardDescription>
					Part des leads qualifiés sur la période dont le verdict est « Écarté
					».
				</CardDescription>
			</CardHeader>
			<CardContent className="flex flex-col gap-0.5">
				<span className="text-2xl font-semibold tabular-nums">
					{formatRatio(discarded.share)}
				</span>
				<span className="text-sm text-muted-foreground">
					{formatNumber(discarded.discarded)} sur{' '}
					{formatNumber(discarded.total)} leads
				</span>
			</CardContent>
		</Card>
	)
}
