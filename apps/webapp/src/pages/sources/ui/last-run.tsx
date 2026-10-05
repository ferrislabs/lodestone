import { runSummary } from '#/pages/sources/lib/state'
import type { Source } from '#/pages/sources/lib/types'

export function LastRun({ source, now }: { source: Source; now: number }) {
	const latest = source.last_runs[0]
	if (!latest) {
		return <span className="text-sm text-muted-foreground">Aucun passage</span>
	}
	const summary = runSummary(latest, now)
	return (
		<div className="flex flex-col gap-0.5" title={summary.absolute}>
			<span
				className={`text-sm font-medium ${summary.failed ? 'text-destructive' : ''}`}
			>
				{summary.statusLabel}
				<span className="font-normal text-muted-foreground">
					{' '}
					· {summary.when}
				</span>
			</span>
			{summary.counts ? (
				<span className="text-xs text-muted-foreground">{summary.counts}</span>
			) : null}
			{summary.error ? (
				<span className="text-xs text-destructive">{summary.error}</span>
			) : null}
			<span className="text-xs text-muted-foreground">{summary.profile}</span>
		</div>
	)
}
