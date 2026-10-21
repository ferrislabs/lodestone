import { formatNumber, formatPercent } from '#/lib/format'
import { pluralize } from '#/lib/labels'
import {
	DISTINCT_SIGNALS,
	type Factor,
	factorTitle,
	formatPoints,
	isValidWeight,
} from '#/pages/profiles/lib/scoring'

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
		origin: 'Candidat accepté par le LLM',
		signal: "Appels d'offres, offres d'emploi",
		strength: 0.6,
	},
	{
		origin: 'Adresse trouvée dans security.txt',
		signal: 'Contact par rôle trouvé',
		strength: 0.7,
	},
	{
		origin: 'Adresse extraite par le LLM',
		signal: 'Contact par rôle trouvé',
		strength: 0.5,
	},
	{
		origin: "Adresse devinée à partir d'un rôle",
		signal: 'Contact par rôle trouvé',
		strength: 0.2,
	},
]

const EXAMPLE_STRENGTH: Readonly<Record<string, number>> = {
	product_detected: 0.9,
	debt_marker: 0.6,
	saas_provider: 0.9,
	role_contact_found: 0.5,
}

const DEFAULT_EXAMPLE_STRENGTH = 0.7

export function exampleStrength(signal: string): number {
	return EXAMPLE_STRENGTH[signal] ?? DEFAULT_EXAMPLE_STRENGTH
}

export function workedExample(factors: readonly Factor[]): string | null {
	const picked = factors
		.filter(
			(factor) =>
				factor.signal !== DISTINCT_SIGNALS && isValidWeight(factor.weight),
		)
		.slice(0, 2)
	const [first, second] = picked
	if (first === undefined || second === undefined) return null
	const parts = [first, second].map((factor) => ({
		title: factorTitle(factor),
		weight: factor.weight ?? 0,
		strength: exampleStrength(factor.signal),
	}))
	const total = parts.reduce(
		(sum, part) => sum + part.weight * part.strength,
		0,
	)
	const [a, b] = parts
	if (a === undefined || b === undefined) return null
	return `Un lead avec un signal « ${a.title} » (force ${formatNumber(a.strength)}) et un signal « ${b.title} » (force ${formatNumber(b.strength)}) : ${a.weight} × ${formatNumber(a.strength)} + ${b.weight} × ${formatNumber(b.strength)} = ${formatPoints(total)} ${pluralize(total, 'point', 'points')}`
}

export function thresholdSummary(
	threshold: number | null,
	points: number | null,
): string | null {
	if (threshold === null || points === null) return null
	return `Seuil actuel : ${formatPercent(threshold / 100)} = ${formatPoints(points)} ${pluralize(points, 'point', 'points')}`
}
