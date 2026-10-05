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
	{ origin: 'HTTP probe', signal: 'Product detected', strength: 0.9 },
	{ origin: 'HTTP probe', signal: 'Debt marker', strength: 0.6 },
	{ origin: 'DNS CNAME', signal: 'SaaS provider', strength: 0.9 },
	{
		origin: 'Keyword candidate accepted by the rules',
		signal: 'Public tenders, job offers',
		strength: 0.7,
	},
	{
		origin: 'Candidate accepted by the LLM',
		signal: 'Public tenders, job offers',
		strength: 0.6,
	},
	{
		origin: 'Address found in security.txt',
		signal: 'Role contact found',
		strength: 0.7,
	},
	{
		origin: 'Address extracted by the LLM',
		signal: 'Role contact found',
		strength: 0.5,
	},
	{
		origin: 'Address guessed from a role',
		signal: 'Role contact found',
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
	return `A lead with a "${a.title}" signal (strength ${a.strength}) and a "${b.title}" signal (strength ${b.strength}): ${a.weight} × ${a.strength} + ${b.weight} × ${b.strength} = ${formatPoints(total)} points`
}

export function thresholdSummary(
	threshold: number | null,
	points: number | null,
): string | null {
	if (threshold === null || points === null) return null
	return `Current threshold: ${threshold}% = ${formatPoints(points)} points`
}
