import { ArrowDownRight, ArrowRight, ArrowUpRight } from 'lucide-react'

import type {
	CountLine,
	HiringView,
	TrendDirection,
} from '#/lib/ecosystem-views'
import { cn } from '#/lib/utils'

interface HiringSummaryProps {
	summary: HiringView
}

const TREND_STYLES: Readonly<Record<TrendDirection, string>> = {
	up: 'text-emerald-700',
	down: 'text-red-700',
	flat: 'text-muted-foreground',
}

function TrendIcon({ direction }: { direction: TrendDirection }) {
	if (direction === 'up') return <ArrowUpRight className="size-4" />
	if (direction === 'down') return <ArrowDownRight className="size-4" />
	return <ArrowRight className="size-4" />
}

function Counts({
	title,
	lines,
	name,
}: {
	title: string
	lines: readonly CountLine[]
	name: string
}) {
	if (lines.length === 0) return null
	return (
		<div className="flex flex-col gap-0.5" data-hiring={name}>
			<span className="text-xs text-muted-foreground">{title}</span>
			<ul className="flex flex-wrap gap-x-3 gap-y-0.5 text-sm">
				{lines.map((line) => (
					<li key={line.label}>
						<span className="font-medium">{line.count}</span> {line.label}
					</li>
				))}
			</ul>
		</div>
	)
}

export function HiringSummary({ summary }: HiringSummaryProps) {
	return (
		<div className="flex flex-col gap-2" data-hiring-summary>
			<div className="flex flex-wrap items-center gap-2">
				<span className="text-sm font-semibold">{summary.headline}</span>
				{summary.trend ? (
					<span
						data-trend={summary.trend.direction}
						className={cn(
							'inline-flex items-center gap-1 text-xs',
							TREND_STYLES[summary.trend.direction],
						)}
					>
						<TrendIcon direction={summary.trend.direction} />
						{summary.trend.label}
					</span>
				) : null}
			</div>
			<Counts title="Métiers" lines={summary.functions} name="functions" />
			<Counts title="Lieux" lines={summary.locations} name="locations" />
			<Counts title="Contrats" lines={summary.contracts} name="contracts" />
			{summary.remote ? (
				<span className="text-xs text-muted-foreground">{summary.remote}</span>
			) : null}
		</div>
	)
}
