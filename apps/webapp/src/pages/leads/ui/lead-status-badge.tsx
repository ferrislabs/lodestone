import { Badge } from '#/components/ui/badge'
import { cn } from '#/lib/utils'
import type { LeadStatus } from '#/pages/leads/lib/types'

const STYLES: Record<LeadStatus, string> = {
	new: 'border-blue-200 bg-blue-50 text-blue-700',
	qualified: 'border-violet-200 bg-violet-50 text-violet-700',
	validated: 'border-emerald-200 bg-emerald-50 text-emerald-700',
	rejected: 'border-red-200 bg-red-50 text-red-700',
	contacted: 'border-teal-200 bg-teal-50 text-teal-700',
	uncertain: 'border-amber-200 bg-amber-50 text-amber-700',
}

interface LeadStatusBadgeProps {
	status: LeadStatus
}

export function LeadStatusBadge({ status }: LeadStatusBadgeProps) {
	return (
		<Badge variant="outline" className={cn('capitalize', STYLES[status])}>
			{status}
		</Badge>
	)
}
