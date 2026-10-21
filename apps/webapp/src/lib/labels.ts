const SIGNAL_KIND_LABELS: Readonly<Record<string, string>> = {
	product_detected: 'Produit détecté',
	debt_marker: 'Marqueur de dette technique',
	saas_provider: 'Fournisseur SaaS',
	job_offer: "Offre d'emploi",
	public_tender: "Appel d'offres",
	awarded_contract: 'Marché attribué',
	role_contact_found: 'Contact par rôle trouvé',
	certificate_seen: 'Certificat vu',
	distinct_signals: 'Types de signaux distincts',
}

const SOURCE_LABELS: Readonly<Record<string, string>> = {
	certificate_transparency: 'Transparence des certificats',
	http_probe: 'Sonde HTTP',
	dns_cname: 'DNS CNAME',
	public_page: 'Page publique',
	security_txt: 'security.txt',
	role_guess: 'Adresse de rôle devinée',
	boamp: 'BOAMP',
	decp: 'DECP',
	ted: 'TED',
	france_travail: 'France Travail',
	ats: "Offres d'emploi de l'entreprise",
	hacker_news_hiring: 'Hacker News — Who is hiring?',
	qualification: 'Qualification et synthèse des leads',
	company_research: 'Recherche entreprise (site et registre officiel)',
}

const CONTACT_METHOD_LABELS: Readonly<Record<string, string>> = {
	public_page: 'Page publique',
	security_txt: 'security.txt',
	role_guess: 'Adresse de rôle devinée',
	llm_extraction: 'Extraction LLM',
}

const CONTACT_KIND_LABELS: Readonly<Record<string, string>> = {
	role: 'Rôle',
	nominative: 'Nominatif',
}

export function signalKindLabel(kind: string): string {
	return SIGNAL_KIND_LABELS[kind] ?? kind
}

export function sourceLabel(source: string): string {
	return SOURCE_LABELS[source] ?? source
}

export function contactMethodLabel(method: string): string {
	return CONTACT_METHOD_LABELS[method] ?? method
}

export function contactKindLabel(kind: string): string {
	return CONTACT_KIND_LABELS[kind] ?? kind
}

export function pluralize(
	count: number,
	singular: string,
	plural: string,
): string {
	return count > 1 ? plural : singular
}
