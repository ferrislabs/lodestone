import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from '#/components/ui/card'
import {
	barHeight,
	bucketLabel,
	histogramMax,
} from '#/pages/quality/lib/format'
import type { Quality } from '#/pages/quality/lib/types'

interface ConfidenceHistogramProps {
	buckets: Quality['confidence']
}

export function ConfidenceHistogram({ buckets }: ConfidenceHistogramProps) {
	const max = histogramMax(buckets)
	return (
		<Card aria-label="Distribution de la confiance">
			<CardHeader>
				<CardTitle>Distribution de la confiance</CardTitle>
				<CardDescription>
					Confiance au moment de la décision, par tranche de 10 %.
				</CardDescription>
			</CardHeader>
			<CardContent className="flex flex-col gap-3">
				<div className="flex items-center gap-4 text-sm">
					<span className="flex items-center gap-1.5">
						<span className="size-3 rounded-sm bg-emerald-500" />
						Validés
					</span>
					<span className="flex items-center gap-1.5">
						<span className="size-3 rounded-sm bg-rose-500" />
						Rejetés
					</span>
				</div>
				<ul className="flex h-40 items-end gap-2">
					{buckets.map((bucket) => (
						<li
							key={bucket.from}
							aria-label={`${bucketLabel(bucket)} : ${bucket.validated} validés, ${bucket.rejected} rejetés`}
							className="flex h-full flex-1 flex-col items-center gap-1"
						>
							<div className="flex w-full flex-1 items-end justify-center gap-0.5">
								<div
									data-bar="validated"
									className="w-1/2 rounded-t-sm bg-emerald-500"
									style={{ height: `${barHeight(bucket.validated, max)}%` }}
								/>
								<div
									data-bar="rejected"
									className="w-1/2 rounded-t-sm bg-rose-500"
									style={{ height: `${barHeight(bucket.rejected, max)}%` }}
								/>
							</div>
							<span className="text-[10px] text-muted-foreground tabular-nums">
								{bucket.from}
							</span>
						</li>
					))}
				</ul>
			</CardContent>
		</Card>
	)
}
