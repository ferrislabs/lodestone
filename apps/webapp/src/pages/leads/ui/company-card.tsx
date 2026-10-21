import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import {
	activityText,
	companyNotice,
	companyState,
	companyStateLabel,
	companyStateVariant,
	formatDay,
	formatSiren,
	headOfficeText,
	ignoredLinksNote,
	pageLabel,
} from '#/pages/leads/lib/company'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { Company } from '#/pages/leads/lib/types'
import { ProvenanceLink } from './provenance-link'

interface CompanyCardProps {
	company: Company | null
	refreshing: boolean
	refreshError: string | null
	onRefresh: () => void
}

function Row({
	label,
	children,
}: {
	label: string
	children: React.ReactNode
}) {
	return (
		<div>
			<dt className="text-muted-foreground">{label}</dt>
			<dd>{children}</dd>
		</div>
	)
}

export function CompanyCard({
	company,
	refreshing,
	refreshError,
	onRefresh,
}: CompanyCardProps) {
	const state = companyState(company)
	const official = company?.official ?? null
	const notice = companyNotice(company)
	const activity = official ? activityText(official) : null
	const headOffice = official ? headOfficeText(official) : null
	const waiting = refreshing || (company !== null && state === 'pending')
	const read = company !== null && state !== 'pending'
	const ignored = read ? ignoredLinksNote(company.ignored_links) : null
	return (
		<section
			data-section="company"
			aria-label="Fiche entreprise"
			className="flex flex-col gap-3 rounded-lg border p-4"
		>
			<div className="flex flex-wrap items-center gap-3">
				<h2 className="text-sm font-semibold">Fiche entreprise</h2>
				<Badge variant={companyStateVariant(state)} data-company-state={state}>
					{companyStateLabel(state)}
				</Badge>
				<Button
					className="ml-auto"
					variant="outline"
					size="sm"
					disabled={waiting}
					onClick={onRefresh}
				>
					{waiting ? 'Enrichissement…' : 'Enrichir'}
				</Button>
			</div>
			{official ? (
				<dl
					data-company-official="true"
					className="grid gap-x-8 gap-y-1 text-sm sm:grid-cols-2 lg:grid-cols-3"
				>
					<Row label="Raison sociale">{official.legal_name}</Row>
					<Row label="SIREN">
						<a
							href={official.annuaire_url}
							target="_blank"
							rel="noopener noreferrer"
							className="text-orange-600 hover:underline"
						>
							{formatSiren(official.siren)}
						</a>
					</Row>
					<Row label="Activité (NAF)">{activity ?? 'Inconnue'}</Row>
					<Row label="Effectif">{official.headcount_range ?? 'Inconnu'}</Row>
					<Row label="Siège">{headOffice ?? 'Inconnu'}</Row>
					<Row label="Création">
						{official.created_on ? formatDay(official.created_on) : 'Inconnue'}
					</Row>
				</dl>
			) : null}
			{notice ? (
				<p data-company-notice="true" className="text-sm text-muted-foreground">
					{notice}
				</p>
			) : null}
			{company?.error ? (
				<p className="text-xs text-muted-foreground">
					Dernière erreur : {company.error}
				</p>
			) : null}
			{read ? (
				<div className="flex flex-col gap-1 text-sm">
					<p className="text-muted-foreground">Pages lues</p>
					{company.pages.length > 0 ? (
						<ul data-company-pages="true" className="flex flex-col gap-0.5">
							{company.pages.map((page) => (
								<li key={page.url} title={pageLabel(page.url)}>
									<ProvenanceLink provenance={page.url} />
								</li>
							))}
						</ul>
					) : (
						<p data-company-no-pages="true">Aucune page exploitable trouvée</p>
					)}
					{ignored ? (
						<p
							data-company-ignored="true"
							className="text-xs text-muted-foreground"
						>
							{ignored}
						</p>
					) : null}
				</div>
			) : null}
			<p className="text-xs text-muted-foreground">
				{company?.built_at
					? `Dernière mise à jour : ${formatDateTime(company.built_at)}`
					: 'Jamais mise à jour'}
			</p>
			{refreshError ? (
				<p role="alert" className="text-sm text-destructive">
					{refreshError}
				</p>
			) : null}
		</section>
	)
}
