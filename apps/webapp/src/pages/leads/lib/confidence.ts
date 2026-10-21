import { formatNumber } from '#/lib/format'
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
	awaiting_scoring: "En attente : aucun agent de scoring n'a répondu",
}

export function verdictReasonLabel(reason: VerdictReason): string {
	return VERDICT_REASON_LABELS[reason]
}

const INDICATOR_CAP_PREFIX = 'Indicateurs plafonnés : '

const INDICATOR_CAP_LABELS: Readonly<Record<VerdictReason, string>> = {
	single_angle: `${INDICATOR_CAP_PREFIX}un seul angle d'analyse`,
	weak_evidence: `${INDICATOR_CAP_PREFIX}preuves insuffisantes`,
	non_production: `${INDICATOR_CAP_PREFIX}uniquement des hôtes hors production`,
	excluded: VERDICT_REASON_LABELS.excluded,
	awaiting_scoring: VERDICT_REASON_LABELS.awaiting_scoring,
}

export function indicatorCapLabel(cap: VerdictReason): string {
	return INDICATOR_CAP_LABELS[cap]
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
	return `Indicateurs : avis ${formatConfidence(lead.opinion)} × preuves ${formatConfidence(lead.evidence.strength)}`
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
		{
			key: 'volume',
			label: 'Volume de signaux indépendants',
			value: components.volume,
		},
		{
			key: 'diversity',
			label: 'Diversité des sources et des types',
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

export interface EvidenceSignalRow {
	key: 'independent' | 'decayed'
	label: string
	value: string
}

export function evidenceSignalRows(evidence: Evidence): EvidenceSignalRow[] {
	return [
		{
			key: 'independent',
			label: 'Signaux indépendants',
			value: `${evidence.independent_signals} sur ${evidence.signal_count}`,
		},
		{
			key: 'decayed',
			label: 'Volume après décroissance',
			value: `${formatNumber(evidence.decayed_signals)} sur ${evidence.independent_signals}`,
		},
	]
}
