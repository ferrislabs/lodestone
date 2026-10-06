import { Button } from '#/components/ui/button'
import { formatScore } from '#/pages/leads/lib/format'
import { leadStatusLabel } from '#/pages/leads/lib/labels'
import type { Company, Lead, LeadStatus } from '#/pages/leads/lib/types'
import { CompanyLine } from './company-line'
import { LeadConfidence } from './lead-confidence'
import { LeadStatusBadge } from './lead-status-badge'
import { VerdictBadge } from './verdict-badge'

interface LeadHeaderProps {
	lead: Lead
	company: Company | null
	onOpenLead: (leadId: string) => void
	nextStatuses: readonly LeadStatus[]
	onChangeStatus: (status: LeadStatus) => void
	changing: boolean
	changeError: string | null
}

export function LeadHeader({
	lead,
	company,
	onOpenLead,
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
				<VerdictBadge verdict={lead.verdict} />
			</div>
			<dl className="grid gap-x-8 gap-y-1 text-sm sm:grid-cols-2 lg:grid-cols-4">
				<div>
					<dt className="text-muted-foreground">Profil</dt>
					<dd>{lead.profile.name}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Score des signaux</dt>
					<dd className="font-mono">{formatScore(lead.score)}</dd>
				</div>
				<LeadConfidence lead={lead} />
				<CompanyLine
					company={company}
					profileId={lead.profile.id}
					onOpenLead={onOpenLead}
				/>
			</dl>
			<div className="flex flex-wrap items-center gap-2">
				<span className="text-sm text-muted-foreground">Passer à</span>
				{nextStatuses.length === 0 ? (
					<span className="text-sm">Aucune autre transition</span>
				) : (
					nextStatuses.map((status) => (
						<Button
							key={status}
							variant="outline"
							size="sm"
							disabled={changing}
							onClick={() => onChangeStatus(status)}
						>
							{leadStatusLabel(status)}
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
