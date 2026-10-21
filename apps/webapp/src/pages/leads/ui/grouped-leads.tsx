import { leadStatusLabel } from '#/pages/leads/lib/labels'
import type { GroupedLead } from '#/pages/leads/lib/types'
import { formatConfidence } from '#/pages/leads/lib/verdict'
import { VerdictBadge } from './verdict-badge'

interface GroupedLeadsProps {
	leads: readonly GroupedLead[]
	onOpenLead: (leadId: string) => void
}

export function GroupedLeads({ leads, onOpenLead }: GroupedLeadsProps) {
	return (
		<ul
			aria-label="Autres domaines de l'entreprise"
			className="flex flex-col gap-1"
		>
			{leads.map((lead) => (
				<li
					key={lead.id}
					data-grouped-lead={lead.id}
					className="flex flex-wrap items-center gap-3 text-sm"
				>
					<a
						href={`/leads/${lead.id}`}
						className="font-medium hover:underline"
						onClick={(event) => {
							event.preventDefault()
							onOpenLead(lead.id)
						}}
					>
						{lead.domain}
					</a>
					<VerdictBadge verdict={lead.verdict} />
					<span className="font-mono text-xs text-muted-foreground">
						{formatConfidence(lead.confidence)}
					</span>
					<span className="text-xs text-muted-foreground">
						{leadStatusLabel(lead.status)}
					</span>
				</li>
			))}
		</ul>
	)
}
