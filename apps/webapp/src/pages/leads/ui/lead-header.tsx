import { Button } from '#/components/ui/button'
import { formatDateTime, formatScore } from '#/pages/leads/lib/format'
import type { Lead, LeadStatus } from '#/pages/leads/lib/types'
import { LeadStatusBadge } from './lead-status-badge'

interface LeadHeaderProps {
	lead: Lead
	nextStatuses: readonly LeadStatus[]
	onChangeStatus: (status: LeadStatus) => void
	changing: boolean
	changeError: string | null
}

export function LeadHeader({
	lead,
	nextStatuses,
	onChangeStatus,
	changing,
	changeError,
}: LeadHeaderProps) {
	return (
		<div className="flex flex-col gap-3">
			<div className="flex flex-wrap items-center gap-3">
				<h1 className="text-2xl font-bold tracking-tight">{lead.domain}</h1>
				<LeadStatusBadge status={lead.status} />
			</div>
			<dl className="grid gap-x-8 gap-y-1 text-sm sm:grid-cols-2 lg:grid-cols-4">
				<div>
					<dt className="text-muted-foreground">Profile</dt>
					<dd>{lead.profile.name}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Score</dt>
					<dd className="font-mono">{formatScore(lead.score)}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">First seen</dt>
					<dd>{formatDateTime(lead.first_seen_at)}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Last seen</dt>
					<dd>{formatDateTime(lead.last_seen_at)}</dd>
				</div>
			</dl>
			<div className="flex flex-wrap items-center gap-2">
				<span className="text-sm text-muted-foreground">Move to</span>
				{nextStatuses.length === 0 ? (
					<span className="text-sm">No further transition</span>
				) : (
					nextStatuses.map((status) => (
						<Button
							key={status}
							variant="outline"
							size="sm"
							disabled={changing}
							onClick={() => onChangeStatus(status)}
							className="capitalize"
						>
							{status}
						</Button>
					))
				)}
			</div>
			{changeError ? (
				<p role="alert" className="text-sm text-destructive">
					{changeError}
				</p>
			) : null}
		</div>
	)
}
