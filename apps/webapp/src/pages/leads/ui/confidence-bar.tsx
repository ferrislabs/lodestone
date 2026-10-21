import { cn } from '#/lib/utils'
import { confidenceWidth, formatConfidence } from '#/pages/leads/lib/verdict'

interface ConfidenceBarProps {
	confidence: number | null | undefined
	breakdown?: string | null
	className?: string
}

export function ConfidenceBar({
	confidence,
	breakdown = null,
	className,
}: ConfidenceBarProps) {
	return (
		<div
			className={cn('flex flex-col gap-0.5', className)}
			title={breakdown ?? undefined}
		>
			<div className="flex items-center gap-2">
				<div
					role="progressbar"
					aria-label="Confiance"
					aria-valuemin={0}
					aria-valuemax={100}
					aria-valuenow={
						confidence == null ? undefined : confidenceWidth(confidence)
					}
					className="h-2 w-24 overflow-hidden rounded-full bg-muted"
				>
					<div
						data-confidence-fill=""
						className="h-full rounded-full bg-orange-500"
						style={{ width: `${confidenceWidth(confidence)}%` }}
					/>
				</div>
				<span className="w-16 text-right font-mono text-xs tabular-nums">
					{formatConfidence(confidence)}
				</span>
			</div>
			{breakdown ? (
				<span
					data-confidence-breakdown=""
					className="text-xs text-muted-foreground"
				>
					{breakdown}
				</span>
			) : null}
		</div>
	)
}
