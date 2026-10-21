import { formatLlmScore, llmScoreWidth } from '#/pages/leads/lib/scoring'

interface LlmScoreBarProps {
	score: number | null | undefined
}

export function LlmScoreBar({ score }: LlmScoreBarProps) {
	return (
		<div className="flex items-center gap-2" data-part="llm-score">
			<div
				role="progressbar"
				aria-label="Score"
				aria-valuemin={0}
				aria-valuemax={100}
				aria-valuenow={score == null ? undefined : llmScoreWidth(score)}
				className="h-2 w-24 overflow-hidden rounded-full bg-muted"
			>
				<div
					className="h-full rounded-full bg-orange-500"
					style={{ width: `${llmScoreWidth(score)}%` }}
				/>
			</div>
			<span className="w-16 text-right font-mono text-xs tabular-nums">
				{formatLlmScore(score)}
			</span>
		</div>
	)
}
