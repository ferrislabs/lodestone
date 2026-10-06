import {
	companyEntity,
	formatSiren,
	siblingLabel,
} from '#/pages/leads/lib/company'
import type { Company } from '#/pages/leads/lib/types'
import { formatConfidence } from '#/pages/leads/lib/verdict'
import { VerdictBadge } from './verdict-badge'

interface CompanyLineProps {
	company: Company | null
	profileId: string
	onOpenLead: (leadId: string) => void
}

export function CompanyLine({
	company,
	profileId,
	onOpenLead,
}: CompanyLineProps) {
	const entity = companyEntity(company)
	if (entity === null || company === null) return null
	return (
		<div
			data-section="company-entity"
			className="flex flex-col gap-1 sm:col-span-2 lg:col-span-4"
		>
			<dt className="text-muted-foreground">Entreprise</dt>
			<dd>
				<span className="font-medium">{entity.legalName}</span>
				<span className="text-muted-foreground">
					{' '}
					· SIREN {formatSiren(entity.siren)}
				</span>
			</dd>
			{company.siblings.length > 0 ? (
				<dd className="flex flex-wrap items-center gap-x-4 gap-y-1">
					<span className="text-muted-foreground">Autres domaines</span>
					<ul className="flex flex-wrap items-center gap-x-4 gap-y-1">
						{company.siblings.map((sibling) => (
							<li
								key={sibling.id}
								data-sibling={sibling.id}
								className="flex items-center gap-1.5"
							>
								<a
									href={`/leads/${sibling.id}`}
									className="text-orange-600 hover:underline"
									onClick={(event) => {
										event.preventDefault()
										onOpenLead(sibling.id)
									}}
								>
									{siblingLabel(sibling, profileId)}
								</a>
								<VerdictBadge verdict={sibling.verdict} />
								<span className="font-mono text-xs text-muted-foreground">
									{formatConfidence(sibling.confidence)}
								</span>
							</li>
						))}
					</ul>
				</dd>
			) : null}
		</div>
	)
}
