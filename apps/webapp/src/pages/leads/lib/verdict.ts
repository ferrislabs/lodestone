import { formatNumber } from '#/lib/format'
import type { Verdict } from '#/pages/leads/lib/types'

export type VerdictKey = Verdict | 'en_attente'

const VERDICT_LABELS: Readonly<Record<VerdictKey, string>> = {
	fiable: 'Fiable',
	a_verifier: 'À vérifier',
	ecarte: 'Écarté',
	en_attente: 'En attente',
}

export function verdictKey(verdict: Verdict | null | undefined): VerdictKey {
	return verdict ?? 'en_attente'
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
