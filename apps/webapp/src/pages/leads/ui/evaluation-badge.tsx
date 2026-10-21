import { cn } from '#/lib/utils'
import type { EvaluationStage } from '#/pages/leads/lib/types'

const STYLES: Readonly<Record<EvaluationStage, string>> = {
	not_evaluated: 'border-slate-200 bg-slate-50 text-slate-600',
	certificate_only: 'border-amber-200 bg-amber-50 text-amber-700',
	requalification_planned: 'border-sky-200 bg-sky-50 text-sky-700',
	inspection_running: 'border-indigo-200 bg-indigo-50 text-indigo-700',
	inspection_complete: 'border-emerald-200 bg-emerald-50 text-emerald-700',
	evaluated: 'border-emerald-200 bg-emerald-50 text-emerald-700',
}

interface EvaluationBadgeProps {
	stage: EvaluationStage | null
	label: string | null
	title?: string | null
}

export function EvaluationBadge({
	stage,
	label,
	title = null,
}: EvaluationBadgeProps) {
	if (stage === null || label === null) return null
	return (
		<span
			data-evaluation={stage}
			title={title ?? undefined}
			className={cn(
				'inline-flex w-fit items-center rounded-md border px-2 py-0.5 text-xs',
				STYLES[stage],
			)}
		>
			{label}
		</span>
	)
}
