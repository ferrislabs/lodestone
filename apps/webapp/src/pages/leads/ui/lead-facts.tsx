import { TriangleAlert } from 'lucide-react'

import { Badge } from '#/components/ui/badge'
import { sourceLabel } from '#/lib/labels'
import { countryBasisLabel } from '#/pages/leads/lib/company'
import { countryName, hostsSummary } from '#/pages/leads/lib/facts'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { LeadFacts as LeadFactsData } from '#/pages/leads/lib/types'

interface LeadFactsProps {
	facts: LeadFactsData
}

export function LeadFacts({ facts }: LeadFactsProps) {
	const country = facts.probable_country ?? null
	return (
		<section
			data-section="facts"
			aria-label="Faits"
			className="flex flex-col gap-2 rounded-lg border p-4"
		>
			<div className="flex flex-wrap items-center gap-3">
				<h2 className="text-sm font-semibold">Faits</h2>
				{facts.only_non_production ? (
					<Badge variant="destructive" data-warning="non-production">
						<TriangleAlert />
						Uniquement des hôtes hors production
					</Badge>
				) : null}
			</div>
			<dl className="grid gap-x-8 gap-y-1 text-sm sm:grid-cols-2 lg:grid-cols-3">
				<div>
					<dt className="text-muted-foreground">Environnements</dt>
					<dd>{hostsSummary(facts.hosts)}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Sous-domaines vus</dt>
					<dd>{facts.distinct_subdomains}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Signaux</dt>
					<dd>{facts.signal_count}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Signaux indépendants</dt>
					<dd>{facts.independent_signals}</dd>
				</div>
				{facts.sibling_domains.length > 0 ? (
					<div data-facts="siblings">
						<dt className="text-muted-foreground">Domaines regroupés</dt>
						<dd>{facts.sibling_domains.join(', ')}</dd>
					</div>
				) : null}
				<div>
					<dt className="text-muted-foreground">Sources</dt>
					<dd>
						{facts.source_count === 0
							? 'Aucune'
							: `${facts.source_count} · ${facts.sources.map(sourceLabel).join(', ')}`}
					</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Première détection</dt>
					<dd>{formatDateTime(facts.first_seen_at)}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Dernière détection</dt>
					<dd>{formatDateTime(facts.last_seen_at)}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">Pays probable</dt>
					<dd>
						{country
							? `${countryName(country.code, country.name)}${facts.country_basis ? ` (${countryBasisLabel(facts.country_basis)})` : ''}`
							: 'Inconnu'}
					</dd>
				</div>
			</dl>
		</section>
	)
}
