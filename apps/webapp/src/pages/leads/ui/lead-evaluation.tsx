import { RefreshCwIcon } from 'lucide-react'

import { Button } from '#/components/ui/button'
import type { ChangedModule } from '#/pages/leads/lib/evaluation'
import type { EvaluationStage } from '#/pages/leads/lib/types'
import { EvaluationBadge } from './evaluation-badge'

interface LeadEvaluationProps {
	stage: EvaluationStage | null
	label: string | null
	evaluatedAt: string | null
	changed: readonly ChangedModule[]
	canReevaluate: boolean
	reevaluating: boolean
	reevaluateError: string | null
	onReevaluate: () => void
}

export function LeadEvaluation({
	stage,
	label,
	evaluatedAt,
	changed,
	canReevaluate,
	reevaluating,
	reevaluateError,
	onReevaluate,
}: LeadEvaluationProps) {
	return (
		<section
			data-section="evaluation"
			aria-label="Évaluation"
			className="flex flex-col gap-2 rounded-lg border p-4"
		>
			<div className="flex flex-wrap items-center justify-between gap-3">
				<div className="flex flex-wrap items-center gap-3">
					<h2 className="text-sm font-semibold">Évaluation</h2>
					<EvaluationBadge stage={stage} label={label} />
				</div>
				<Button
					type="button"
					variant="outline"
					size="sm"
					disabled={!canReevaluate || reevaluating}
					onClick={onReevaluate}
				>
					<RefreshCwIcon />
					Réévaluer
				</Button>
			</div>
			{evaluatedAt ? (
				<p className="text-xs text-muted-foreground">{evaluatedAt}</p>
			) : null}
			{changed.length > 0 ? (
				<div className="flex flex-col gap-1">
					<p className="text-sm">Nouveautés depuis la dernière évaluation :</p>
					<ul className="flex flex-col gap-0.5 text-sm">
						{changed.map((module) => (
							<li key={module.module} data-changed-module={module.module}>
								{module.label}
								{module.at ? (
									<span className="text-xs text-muted-foreground">
										{' '}
										({module.at})
									</span>
								) : null}
							</li>
						))}
					</ul>
				</div>
			) : null}
			{reevaluateError ? (
				<p role="alert" className="text-sm text-destructive">
					Réévaluation impossible : {reevaluateError}
				</p>
			) : null}
		</section>
	)
}
