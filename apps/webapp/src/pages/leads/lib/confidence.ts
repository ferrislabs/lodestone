import type { Exclusion, Lead } from '#/pages/leads/lib/types'
import { formatConfidence } from '#/pages/leads/lib/verdict'

type Evidence = NonNullable<Lead['evidence']>
export type VerdictReason = NonNullable<Lead['verdict_reason']>

const CAP_PREFIX = 'Plafonné à « À vérifier » : '

const VERDICT_REASON_LABELS: Readonly<Record<VerdictReason, string>> = {
	single_angle: `${CAP_PREFIX}un seul angle d'analyse`,
	weak_evidence: `${CAP_PREFIX}preuves insuffisantes`,
	non_production: `${CAP_PREFIX}uniquement des hôtes hors production`,
	excluded: 'Écarté : un agent de qualification a exclu le lead',
}

export function verdictReasonLabel(reason: VerdictReason): string {
	return VERDICT_REASON_LABELS[reason]
}

export function exclusionTitle(exclusion: Exclusion): string {
	return `Exclu par l'agent « ${exclusion.agent_name} » : ${exclusion.label} (${formatConfidence(exclusion.probability)})`
}

export function exclusionSummary(
	exclusions: readonly Exclusion[],
): string | null {
	const [first] = exclusions
	if (!first) return null
	const more = exclusions.length - 1
	return more > 0
		? `Exclu : ${first.label} (+${more})`
		: `Exclu : ${first.label}`
}

export function confidenceBreakdown(
	lead: Pick<Lead, 'opinion' | 'evidence'>,
): string | null {
	if (lead.opinion == null || lead.evidence == null) return null
	return `Avis ${formatConfidence(lead.opinion)} · Preuves ${formatConfidence(lead.evidence.strength)}`
}

export interface EvidenceComponentRow {
	key: keyof Evidence['components']
	label: string
	value: number
}

export function evidenceComponentRows(
	evidence: Evidence,
): EvidenceComponentRow[] {
	const { components } = evidence
	return [
		{ key: 'volume', label: 'Volume de signaux', value: components.volume },
		{
			key: 'diversity',
			label: 'Diversité des sources',
			value: components.diversity,
		},
		{
			key: 'environment',
			label: 'Environnement',
			value: components.environment,
		},
		{ key: 'strength', label: 'Force des signaux', value: components.strength },
	]
}
