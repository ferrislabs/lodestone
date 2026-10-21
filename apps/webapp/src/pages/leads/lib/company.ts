import type {
	Company,
	CompanyOfficial,
	GroupedLead,
	SiblingLead,
} from './types'

export type CompanyState = 'pending' | 'complete' | 'partial' | 'unreachable'

const STATE_LABELS: Readonly<Record<CompanyState, string>> = {
	pending: 'Recherche en attente',
	complete: 'Complète',
	partial: 'Partielle',
	unreachable: 'Inaccessible',
}

export function companyState(company: Company | null): CompanyState {
	switch (company?.state) {
		case 'complete':
		case 'partial':
		case 'unreachable':
			return company.state
		default:
			return 'pending'
	}
}

export function companyStateLabel(state: CompanyState): string {
	return STATE_LABELS[state]
}

export function companyStateVariant(
	state: CompanyState,
): 'default' | 'secondary' | 'destructive' | 'outline' {
	switch (state) {
		case 'complete':
			return 'default'
		case 'partial':
			return 'secondary'
		case 'unreachable':
			return 'destructive'
		default:
			return 'outline'
	}
}

export function formatSiren(siren: string): string {
	const digits = siren.replace(/\s+/g, '')
	if (!/^\d{9}$/.test(digits)) return siren
	return `${digits.slice(0, 3)} ${digits.slice(3, 6)} ${digits.slice(6)}`
}

export function activityText(official: CompanyOfficial): string | null {
	const parts = [official.naf_code, official.naf_label].filter(
		(part): part is string => Boolean(part),
	)
	return parts.length === 0 ? null : parts.join(' · ')
}

export function headOfficeText(official: CompanyOfficial): string | null {
	const place = [official.head_office_postcode, official.head_office_city]
		.filter((part): part is string => Boolean(part))
		.join(' ')
	const parts = [place, official.head_office_country].filter(
		(part): part is string => Boolean(part),
	)
	return parts.length === 0 ? null : parts.join(', ')
}

export function formatDay(day: string): string {
	const date = new Date(`${day}T00:00:00Z`)
	if (Number.isNaN(date.getTime())) return day
	return new Intl.DateTimeFormat('fr-FR', {
		dateStyle: 'long',
		timeZone: 'UTC',
	}).format(date)
}

export function companyNotice(company: Company | null): string | null {
	const state = companyState(company)
	if (company === null || state === 'pending') {
		return "La recherche n'a pas encore été faite pour ce domaine."
	}
	if (state === 'unreachable') {
		return "Le site de l'entreprise est inaccessible ou interdit aux robots."
	}
	if (state === 'partial' && !company.official) {
		return company.siren
			? `Le registre officiel n'a pas répondu pour le SIREN ${formatSiren(company.siren)}.`
			: 'SIREN introuvable sur le site de l’entreprise : aucune donnée officielle.'
	}
	return null
}

export function ignoredLinksNote(count: number): string | null {
	if (count <= 0) {
		return null
	}
	return count === 1
		? '1 lien ignoré : page introuvable, vide ou redirigée vers l’accueil.'
		: `${count} liens ignorés : pages introuvables, vides ou redirigées vers l’accueil.`
}

export function pageLabel(url: string): string {
	try {
		const parsed = new URL(url)
		const path = parsed.pathname === '/' ? '' : parsed.pathname
		return `${parsed.host}${path}`
	} catch {
		return url
	}
}

const COUNTRY_BASIS_LABELS: Readonly<Record<string, string>> = {
	head_office: 'siège',
	tld: 'extension du domaine',
}

export function countryBasisLabel(basis: string | null | undefined): string {
	return basis ? (COUNTRY_BASIS_LABELS[basis] ?? basis) : ''
}

export interface CompanyEntity {
	legalName: string
	siren: string
}

export function companyEntity(company: Company | null): CompanyEntity | null {
	if (!company?.legal_name || !company.siren) return null
	return { legalName: company.legal_name, siren: company.siren }
}

export function siblingLabel(
	sibling: SiblingLead,
	currentProfileId: string,
): string {
	return sibling.profile.id === currentProfileId
		? sibling.domain
		: `${sibling.domain} · ${sibling.profile.name}`
}

export function groupedBadge(members: readonly GroupedLead[]): string | null {
	if (members.length === 0) return null
	return `+${members.length} ${members.length > 1 ? 'domaines' : 'domaine'}`
}
