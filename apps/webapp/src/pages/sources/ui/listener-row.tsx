import { Badge } from '#/components/ui/badge'
import { TableCell, TableRow } from '#/components/ui/table'
import { formatDateTime } from '#/lib/format'
import {
	backlogText,
	lastActivityText,
	lastUpdateText,
	latestError,
	listenerBadge,
	STALE_EXPLANATION,
	totalsText,
} from '#/pages/sources/lib/listener'
import { shortError, sourceTitle } from '#/pages/sources/lib/state'
import type { Listener, Source } from '#/pages/sources/lib/types'
import { LastRun } from '#/pages/sources/ui/last-run'
import { ProviderNotice } from '#/pages/sources/ui/provider-notice'

interface ListenerRowProps {
	source: Source
	listener: Listener
	now: number
	onOpenProvider: (key: string) => void
}

function badgeVariant(tone: ReturnType<typeof listenerBadge>['tone']) {
	switch (tone) {
		case 'error':
			return 'destructive'
		case 'active':
			return 'default'
		case 'standby':
		case 'stale':
			return 'secondary'
		default:
			return 'outline'
	}
}

export function ListenerRow({
	source,
	listener,
	now,
	onOpenProvider,
}: ListenerRowProps) {
	const badge = listenerBadge(listener.state)
	const error = latestError(listener)
	return (
		<TableRow data-source={source.source} data-listener={listener.state}>
			<TableCell className="font-medium">{sourceTitle(source)}</TableCell>
			<TableCell>
				<div className="flex flex-col items-start gap-1">
					<Badge variant={badgeVariant(badge.tone)}>{badge.label}</Badge>
					{error ? (
						<span
							data-part="listener-error"
							className="max-w-56 text-xs text-destructive"
						>
							<span>{shortError(error.message)}</span>{' '}
							<time dateTime={error.at} className="text-muted-foreground">
								({formatDateTime(error.at)})
							</time>
						</span>
					) : null}
					<ProviderNotice source={source} onOpenProvider={onOpenProvider} />
					{listener.state === 'stale' ? (
						<span className="max-w-56 text-xs text-muted-foreground">
							{lastUpdateText(listener.updated_at, now)}. {STALE_EXPLANATION}.
						</span>
					) : null}
				</div>
			</TableCell>
			<TableCell>
				<div className="flex flex-col">
					<span className="text-sm">
						{lastActivityText(listener.last_activity_at, now)}
					</span>
					{listener.last_activity_at ? (
						<time
							dateTime={listener.last_activity_at}
							className="text-xs text-muted-foreground"
						>
							{formatDateTime(listener.last_activity_at)}
						</time>
					) : null}
				</div>
			</TableCell>
			<TableCell>
				{listener.backlog == null ? (
					<span className="text-sm text-muted-foreground">—</span>
				) : (
					<span className="text-sm">
						En attente : {backlogText(source.source, listener.backlog)}
					</span>
				)}
			</TableCell>
			<TableCell>
				<span className="text-sm">{totalsText(listener)}</span>
			</TableCell>
			<TableCell>
				<LastRun source={source} now={now} />
			</TableCell>
		</TableRow>
	)
}
