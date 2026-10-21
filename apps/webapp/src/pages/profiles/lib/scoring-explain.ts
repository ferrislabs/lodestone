export interface StrengthRow {
	origin: string
	signal: string
	strength: number
}

export const STRENGTH_ROWS: readonly StrengthRow[] = [
	{ origin: 'Sonde HTTP', signal: 'Produit détecté', strength: 0.9 },
	{
		origin: 'Sonde HTTP',
		signal: 'Marqueur de dette technique',
		strength: 0.6,
	},
	{ origin: 'DNS CNAME', signal: 'Fournisseur SaaS', strength: 0.9 },
	{
		origin: 'Candidat par mots-clés accepté par les règles',
		signal: "Appels d'offres, offres d'emploi",
		strength: 0.7,
	},
	{
		origin: 'Candidat par mots-clés sous le minimum',
		signal: "Appels d'offres, offres d'emploi",
		strength: 0.5,
	},
	{
		origin: 'Certificate Transparency',
		signal: 'Certificat observé',
		strength: 0.3,
	},
]
