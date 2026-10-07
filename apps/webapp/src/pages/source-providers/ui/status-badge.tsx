import { Badge } from '#/components/ui/badge'
import { cn } from '#/lib/utils'
import { statusLabel } from '#/pages/source-providers/lib/labels'

const TONES: Readonly<Record<string, string>> = {
	ready:
		'border-transparent bg-emerald-100 text-emerald-800 dark:bg-emerald-500/15 dark:text-emerald-300',
	untested:
		'border-transparent bg-amber-100 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300',
	failing: 'border-transparent bg-destructive text-white',
	not_configured: 'text-muted-foreground',
}

interface StatusBadgeProps {
	status: string
}

export function StatusBadge({ status }: StatusBadgeProps) {
	return (
		<Badge
			variant="outline"
			className={cn(TONES[status] ?? TONES.not_configured)}
		>
			{statusLabel(status)}
		</Badge>
	)
}
