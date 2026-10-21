import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { TableCell, TableRow } from '#/components/ui/table'
import { formatDateTime, formatRelative } from '#/lib/format'
import {
	canRun,
	runDisabledReason,
	sourceState,
	sourceTitle,
	stateLabel,
} from '#/pages/sources/lib/state'
import type { Source, SourceState } from '#/pages/sources/lib/types'
import { LastRun } from '#/pages/sources/ui/last-run'

interface SourceRowProps {
	source: Source
	now: number
	scheduled: boolean
	pending: boolean
	error: string | null
	onRun: (source: string) => void
}

function badgeVariant(state: SourceState) {
	switch (state) {
		case 'failed':
			return 'destructive'
		case 'running':
			return 'default'
		case 'pending':
			return 'secondary'
		default:
			return 'outline'
	}
}

function NextRun({ source, now }: { source: Source; now: number }) {
	if (sourceState(source) === 'running' && source.running_since) {
		return (
			<span className="text-sm">
				Démarré {formatRelative(source.running_since, now)}
			</span>
		)
	}
	if (sourceState(source) !== 'pending' || !source.next_run_at) {
		return <span className="text-sm text-muted-foreground">—</span>
	}
	return (
		<div className="flex flex-col">
			<span className="text-sm">{formatRelative(source.next_run_at, now)}</span>
			<time
				dateTime={source.next_run_at}
				className="text-xs text-muted-foreground"
			>
				{formatDateTime(source.next_run_at)}
			</time>
		</div>
	)
}

function buttonLabel(scheduled: boolean, pending: boolean): string {
	if (pending) return 'Planification…'
	if (scheduled) return 'Planifié maintenant'
	return 'Lancer maintenant'
}

export function SourceRow({
	source,
	now,
	scheduled,
	pending,
	error,
	onRun,
}: SourceRowProps) {
	const state = sourceState(source)
	const reason = runDisabledReason(source)
	const busy = pending || scheduled
	const disabled = !canRun(source) || busy
	const explanationId = `source-${source.source}-note`
	const note = error ?? (busy ? null : reason)
	return (
		<TableRow data-source={source.source} data-state={state}>
			<TableCell className="font-medium">{sourceTitle(source)}</TableCell>
			<TableCell>
				<Badge variant={badgeVariant(state)}>{stateLabel(state)}</Badge>
			</TableCell>
			<TableCell>
				<NextRun source={source} now={now} />
			</TableCell>
			<TableCell>
				<LastRun source={source} now={now} />
			</TableCell>
			<TableCell>
				<div className="flex flex-col items-start gap-1">
					<Button
						type="button"
						size="sm"
						variant="outline"
						disabled={disabled}
						aria-describedby={note ? explanationId : undefined}
						title={reason ?? undefined}
						onClick={() => onRun(source.source)}
					>
						{buttonLabel(scheduled, pending)}
					</Button>
					{note ? (
						<span
							id={explanationId}
							role={error ? 'alert' : undefined}
							className={`max-w-56 text-xs ${error ? 'text-destructive' : 'text-muted-foreground'}`}
						>
							{note}
						</span>
					) : null}
				</div>
			</TableCell>
		</TableRow>
	)
}
