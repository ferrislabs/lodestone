import { Badge } from '#/components/ui/badge'
import { cn } from '#/lib/utils'
import { PENDING_SCORING_LABEL } from '#/pages/leads/lib/scoring'
import type { Verdict } from '#/pages/leads/lib/types'
import {
	type VerdictKey,
	verdictKey,
	verdictLabel,
} from '#/pages/leads/lib/verdict'

const STYLES: Record<VerdictKey, string> = {
	fiable: 'border-emerald-200 bg-emerald-50 text-emerald-700',
	a_verifier: 'border-amber-200 bg-amber-50 text-amber-700',
	ecarte: 'border-red-200 bg-red-50 text-red-700',
	en_attente: 'border-slate-200 bg-slate-50 text-slate-600',
}

interface VerdictBadgeProps {
	verdict: Verdict | null | undefined
}

export function VerdictBadge({ verdict }: VerdictBadgeProps) {
	return (
		<Badge
			variant="outline"
			data-verdict={verdictKey(verdict)}
			className={cn(STYLES[verdictKey(verdict)])}
		>
			{verdict ? verdictLabel(verdict) : PENDING_SCORING_LABEL}
		</Badge>
	)
}
