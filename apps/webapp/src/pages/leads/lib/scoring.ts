import { formatNumber } from '#/lib/format'
import type {
	ScoringResult,
	ScoringVerdict,
	Verdict,
	VerdictProbabilities,
} from '#/pages/leads/lib/types'
import { verdictLabel } from '#/pages/leads/lib/verdict'

export const MAX_LLM_SCORE = 100
export const PENDING_SCORING_LABEL = 'En attente du scoring'

export interface VerdictProbabilityRow {
	key: Verdict
	label: string
	percent: number
}

const VERDICT_ORDER: readonly Verdict[] = ['fiable', 'a_verifier', 'ecarte']

export function formatLlmScore(score: number | null | undefined): string {
	return score == null ? '—' : `${formatNumber(score)} / ${MAX_LLM_SCORE}`
}

export function llmScoreWidth(score: number | null | undefined): number {
	if (score == null) return 0
	return Math.round(Math.min(MAX_LLM_SCORE, Math.max(0, score)))
}

export function verdictProbabilityRows(
	probabilities: VerdictProbabilities | null | undefined,
): VerdictProbabilityRow[] {
	if (probabilities == null) return []
	return VERDICT_ORDER.map((key) => ({
		key,
		label: verdictLabel(key),
		percent: probabilities[key],
	}))
}

export function formatPercentValue(percent: number): string {
	return `${formatNumber(percent)} %`
}

export function declaredChoiceNote(verdict: ScoringVerdict): string | null {
	if (verdict.declared_choice == null) return null
	return `Verdict annoncé par le modèle : ${verdictLabel(verdict.declared_choice)}, ramené à ses probabilités`
}

export function scoringProviderText(result: ScoringResult): string {
	const name = result.provider_name
	if (name && result.model) return `${name} (${result.model})`
	return name ?? (result.model || 'Fournisseur inconnu')
}

export function scoringWeightLabel(weight: number | null | undefined): string {
	return weight == null ? '' : `poids ${weight}`
}
