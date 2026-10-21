import { formatNumber } from '#/lib/format'
import type { Verdict } from '#/pages/leads/lib/types'

export type VerdictKey = Verdict | 'en_attente'

export const VERDICT_KEYS: readonly VerdictKey[] = [
	'fiable',
	'a_verifier',
	'ecarte',
	'en_attente',
]

export const DEFAULT_VERDICTS: readonly VerdictKey[] = [
	'fiable',
	'a_verifier',
	'en_attente',
]

const EXCERPT_LENGTH = 140

const VERDICT_LABELS: Readonly<Record<VerdictKey, string>> = {
	fiable: 'Fiable',
	a_verifier: 'À vérifier',
	ecarte: 'Écarté',
	en_attente: 'En attente',
}

export function verdictKey(verdict: Verdict | null | undefined): VerdictKey {
	return verdict ?? 'en_attente'
}

export function verdictKeyLabel(key: VerdictKey): string {
	return VERDICT_LABELS[key]
}

export function verdictLabel(verdict: Verdict | null | undefined): string {
	return VERDICT_LABELS[verdictKey(verdict)]
}

export function formatConfidence(confidence: number | null | undefined) {
	return confidence == null ? '—' : `${formatNumber(confidence)} %`
}

export function formatAgentScore(score: number | null | undefined): string {
	return score == null ? '—' : formatProbability(score)
}

export function formatProbability(probability: number): string {
	return `${formatNumber(Math.round(probability * 1000) / 10)} %`
}

export function probabilityWidth(probability: number): number {
	return Math.round(Math.min(1, Math.max(0, probability)) * 100)
}

export function confidenceWidth(confidence: number | null | undefined): number {
	if (confidence == null) return 0
	return Math.round(Math.min(100, Math.max(0, confidence)))
}

export function summaryExcerpt(
	summary: string | null | undefined,
	max: number = EXCERPT_LENGTH,
): string {
	const text = (summary ?? '').replace(/\s+/g, ' ').trim()
	if (text.length <= max) return text
	return `${text.slice(0, max - 1).trimEnd()}…`
}

export function isDefaultVerdicts(verdicts: readonly VerdictKey[]): boolean {
	return (
		verdicts.length === DEFAULT_VERDICTS.length &&
		DEFAULT_VERDICTS.every((key) => verdicts.includes(key))
	)
}

export function orderedVerdicts(verdicts: readonly VerdictKey[]): VerdictKey[] {
	return VERDICT_KEYS.filter((key) => verdicts.includes(key))
}
