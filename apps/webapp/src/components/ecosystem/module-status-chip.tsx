import { Badge } from '#/components/ui/badge'
import type { StatusTone } from '#/lib/ecosystem'
import { cn } from '#/lib/utils'

const TONE_STYLES: Readonly<Record<StatusTone, string>> = {
	done: 'border-emerald-200 bg-emerald-50 text-emerald-700',
	pending: 'border-amber-200 bg-amber-50 text-amber-700',
	skipped: 'border-slate-200 bg-slate-50 text-slate-600',
	failed: 'border-red-200 bg-red-50 text-red-700',
	off: 'border-slate-200 bg-slate-50 text-slate-500',
	soon: 'border-sky-200 bg-sky-50 text-sky-700',
}

interface ModuleStatusChipProps {
	label: string
	tone: StatusTone
}

export function ModuleStatusChip({ label, tone }: ModuleStatusChipProps) {
	return (
		<Badge
			variant="outline"
			data-tone={tone}
			className={cn('shrink-0', TONE_STYLES[tone])}
		>
			{label}
		</Badge>
	)
}
