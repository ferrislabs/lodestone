import { formatScore, scoreRatio } from '#/pages/leads/lib/format'

interface ScoreBarProps {
	score: number
	max: number
}

export function ScoreBar({ score, max }: ScoreBarProps) {
	const percent = Math.round(scoreRatio(score, max) * 100)
	return (
		<div className="flex items-center gap-2">
			<div className="h-2 w-24 overflow-hidden rounded-full bg-muted">
				<div
					className="h-full rounded-full bg-orange-500"
					style={{ width: `${percent}%` }}
				/>
			</div>
			<span className="w-10 text-right font-mono text-xs tabular-nums">
				{formatScore(score)}
			</span>
		</div>
	)
}
